use anyhow::{Context, Result, bail};
use flate2::{Compression, write::GzEncoder};
use fo2_dps::{
    character, db, effects, encounter, grinding, mobs, pages, scraper, storage, validation, web,
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::Duration;

use fo2_dps::db::{Item, ItemSet, ScrapeFailure, ScrapeReport, Skill, load_skills};

const BULK_REQUEST_DELAY: Duration = Duration::from_millis(200);

#[derive(Serialize)]
struct MobProvenance {
    schema_version: u32,
    record_count: usize,
    normalized_path: &'static str,
    normalized_sha256: String,
    archive_path: &'static str,
    archive_format: &'static str,
    archive_sha256: String,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    match args.as_slice() {
        [_, command, slug] if command == "item" => {
            let item = scraper::fetch_item(slug)?;

            println!();
            println!("{item:#?}");
        }

        [_, command, slug] if command == "skill" => {
            let skill = scraper::fetch_skill(slug)?;

            println!();
            println!("{skill:#?}");
        }

        [_, command, slug] if command == "item-set" => {
            let item_set = scraper::fetch_item_set(slug)?;

            println!();
            println!("{item_set:#?}");
        }

        [_, command] if command == "items" => collect_items()?,

        [_, command] if command == "items-audit" => collect_set_piece_items()?,

        [_, command] if command == "skills" => collect_skills()?,

        [_, command, slug] if command == "mob" => {
            println!("{}", serde_json::to_string_pretty(&mobs::fetch_mob(slug)?)?);
        }

        [_, command] if command == "mobs" => collect_mobs()?,

        [_, command] if command == "normalize-data" => normalize_data()?,

        [_, command] if command == "validate-data" => validation::validate_data()?,

        [_, command] if command == "build-db" => storage::build_database()?,

        [_, command] if command == "build-web-bundle" => pages::build_web_bundle()?,

        [_, command] if command == "assemble-pages" => pages::assemble_pages()?,

        [_, command, path] if command == "inspect-build" => inspect_build_file(path)?,

        [_, command, build_path, mob_slug, assumptions_path] if command == "encounter" => {
            run_encounter(build_path, mob_slug, assumptions_path)?
        }

        [_, command, build_path, mob_slug, assumptions_path] if command == "grind" => {
            run_grind(build_path, mob_slug, assumptions_path)?
        }

        [_, command, build_path, comparison_path] if command == "grind-compare" => {
            run_grind_comparison(build_path, comparison_path)?
        }

        [_, command] if command == "serve" => web::serve("127.0.0.1:8787")?,

        [_, command, address] if command == "serve" => web::serve(address)?,

        [_, command] if command == "item-sets" => {
            let slugs = scraper::fetch_item_set_slugs()?;
            if slugs.is_empty() {
                bail!("No item sets discovered; refusing an empty collection");
            }
            let mut item_sets = Vec::new();

            for slug in slugs {
                item_sets.push(scraper::fetch_item_set(&slug)?);
            }

            let json = serde_json::to_string_pretty(&item_sets)?;
            atomic_write(std::path::Path::new("data/item-sets.json"), json.as_bytes())?;

            println!("Saved {} item sets to data/item-sets.json", item_sets.len());
        }

        [_, command, slug] if command == "item-debug" => {
            scraper::debug_item(slug)?;
        }

        _ => {
            bail!(
                "Usage:\n\
                 \n\
                 fo2-dps item <slug>\n\
                 fo2-dps skill <slug>\n\
                 fo2-dps item-set <slug>\n\
                 fo2-dps items\n\
                 fo2-dps items-audit\n\
                 fo2-dps skills\n\
                 fo2-dps item-sets\n\
                 fo2-dps normalize-data\n\
                 fo2-dps mob <slug>\n\
                 fo2-dps mobs\n\
                 fo2-dps validate-data\n\
                 fo2-dps build-db\n\
                 fo2-dps build-web-bundle\n\
                 fo2-dps assemble-pages\n\
                 fo2-dps inspect-build <build.json>\n\
                 fo2-dps encounter <build.json> <mob-slug> <assumptions.json>\n\
                 fo2-dps grind <build.json> <mob-slug> <assumptions.json>\n\
                 fo2-dps grind-compare <build.json> <comparison.json>\n\
                 fo2-dps serve [address]"
            );
        }
    }

    Ok(())
}

fn run_grind_comparison(build_path: &str, comparison_path: &str) -> Result<()> {
    let build: character::CharacterBuild = serde_json::from_slice(&fs::read(build_path)?)
        .with_context(|| format!("Could not load character build from {build_path}"))?;
    let mut comparison: grinding::GrindingComparison =
        serde_json::from_slice(&fs::read(comparison_path)?).with_context(|| {
            format!("Could not load grinding comparison from {comparison_path}")
        })?;
    let items: Vec<Item> = serde_json::from_slice(&fs::read("data/items.json")?)
        .context("Could not load data/items.json")?;
    let item_sets: Vec<ItemSet> = serde_json::from_slice(&fs::read("data/item-sets.json")?)
        .context("Could not load data/item-sets.json")?;
    let skills = load_skills()?;
    let mobs: Vec<mobs::Mob> = serde_json::from_slice(&fs::read("data/mobs.json")?)
        .context("Could not load data/mobs.json")?;
    let build_inspection = character::inspect_build(&build, &items, &item_sets, &skills);
    if !build_inspection.is_valid() {
        println!("{}", serde_json::to_string_pretty(&build_inspection)?);
        bail!("Character build has validation errors");
    }
    for target in &mut comparison.targets {
        target.assumptions.encounter.apply_build_defense(
            build_inspection.level,
            build_inspection.confirmed_derived_stats.armor,
        );
    }
    let result = grinding::compare_grinding(&mobs, &items, comparison)
        .map_err(anyhow::Error::msg)
        .context("Could not compare grinding targets")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "build": build_inspection,
            "comparison": result
        }))?
    );
    Ok(())
}

fn run_grind(build_path: &str, mob_slug: &str, assumptions_path: &str) -> Result<()> {
    let build: character::CharacterBuild = serde_json::from_slice(&fs::read(build_path)?)
        .with_context(|| format!("Could not load character build from {build_path}"))?;
    let mut assumptions: grinding::GrindingAssumptions =
        serde_json::from_slice(&fs::read(assumptions_path)?).with_context(|| {
            format!("Could not load grinding assumptions from {assumptions_path}")
        })?;
    let items: Vec<Item> = serde_json::from_slice(&fs::read("data/items.json")?)
        .context("Could not load data/items.json")?;
    let item_sets: Vec<ItemSet> = serde_json::from_slice(&fs::read("data/item-sets.json")?)
        .context("Could not load data/item-sets.json")?;
    let skills = load_skills()?;
    let mob_records: Vec<mobs::Mob> = serde_json::from_slice(&fs::read("data/mobs.json")?)
        .context("Could not load data/mobs.json")?;
    let mob = mob_records
        .iter()
        .find(|mob| mob.slug == mob_slug)
        .with_context(|| format!("Mob slug {mob_slug} is absent from data/mobs.json"))?;
    let build_inspection = character::inspect_build(&build, &items, &item_sets, &skills);
    if !build_inspection.is_valid() {
        println!("{}", serde_json::to_string_pretty(&build_inspection)?);
        bail!("Character build has validation errors");
    }
    assumptions.encounter.apply_build_defense(
        build_inspection.level,
        build_inspection.confirmed_derived_stats.armor,
    );
    let estimate = grinding::estimate_grinding(mob, &items, assumptions)
        .map_err(anyhow::Error::msg)
        .context("Could not estimate grinding")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "build": build_inspection,
            "grinding": estimate
        }))?
    );
    Ok(())
}

fn run_encounter(build_path: &str, mob_slug: &str, assumptions_path: &str) -> Result<()> {
    let build: character::CharacterBuild = serde_json::from_slice(&fs::read(build_path)?)
        .with_context(|| format!("Could not load character build from {build_path}"))?;
    let mut assumptions: encounter::EncounterAssumptions =
        serde_json::from_slice(&fs::read(assumptions_path)?).with_context(|| {
            format!("Could not load grinding assumptions from {assumptions_path}")
        })?;
    let items: Vec<Item> = serde_json::from_slice(&fs::read("data/items.json")?)
        .context("Could not load data/items.json")?;
    let item_sets: Vec<ItemSet> = serde_json::from_slice(&fs::read("data/item-sets.json")?)
        .context("Could not load data/item-sets.json")?;
    let skills = load_skills()?;
    let mob_records: Vec<mobs::Mob> = serde_json::from_slice(&fs::read("data/mobs.json")?)
        .context("Could not load data/mobs.json")?;
    let mob = mob_records
        .iter()
        .find(|mob| mob.slug == mob_slug)
        .with_context(|| format!("Mob slug {mob_slug} is absent from data/mobs.json"))?;

    let build_inspection = character::inspect_build(&build, &items, &item_sets, &skills);
    if !build_inspection.is_valid() {
        println!("{}", serde_json::to_string_pretty(&build_inspection)?);
        bail!("Character build has validation errors");
    }
    assumptions.apply_build_defense(
        build_inspection.level,
        build_inspection.confirmed_derived_stats.armor,
    );
    let estimate = encounter::estimate_encounter(mob, assumptions)
        .map_err(anyhow::Error::msg)
        .context("Could not estimate encounter")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "build": build_inspection,
            "mob": {
                "slug": mob.slug,
                "name": mob.name,
                "level": mob.level,
                "published_health": mob.health,
                "published_damage": mob.damage,
                "published_attack_speed_ms": mob.attack_speed_ms,
                "attacks": mob.attacks
            },
            "encounter": estimate
        }))?
    );
    Ok(())
}

fn inspect_build_file(path: &str) -> Result<()> {
    let build: character::CharacterBuild = serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("Could not load character build from {path}"))?;
    let items: Vec<Item> = serde_json::from_slice(&fs::read("data/items.json")?)
        .context("Could not load data/items.json")?;
    let item_sets: Vec<ItemSet> = serde_json::from_slice(&fs::read("data/item-sets.json")?)
        .context("Could not load data/item-sets.json")?;
    let skills = load_skills()?;
    let inspection = character::inspect_build(&build, &items, &item_sets, &skills);
    println!("{}", serde_json::to_string_pretty(&inspection)?);
    if !inspection.is_valid() {
        bail!("Character build has validation errors");
    }
    Ok(())
}

fn normalize_data() -> Result<()> {
    let mut pending = Vec::new();
    let mut item_reports = BTreeMap::new();
    for path in ["data/items.json", "data/item-audit.json"] {
        if !std::path::Path::new(path).exists() {
            continue;
        }
        let mut items: Vec<Item> = serde_json::from_slice(&fs::read(path)?)
            .with_context(|| format!("Could not load {path}"))?;
        let mut consumed = 0;
        let mut unknown = BTreeMap::<String, usize>::new();
        for item in &mut items {
            consumed += scraper::normalize_item_from_unparsed_lines(item);
            for line in &item.unparsed_tooltip_lines {
                *unknown.entry(line.clone()).or_default() += 1;
            }
        }
        item_reports.insert(path, serde_json::json!({"records": items.len(), "lines_normalized_this_run": consumed, "remaining_lines": unknown}));
        pending.push((path, serde_json::to_vec_pretty(&items)?));
    }
    let mut skills: Vec<Skill> = serde_json::from_slice(&fs::read("data/skills.json")?)?;
    let mut unknown_effects = BTreeMap::<String, usize>::new();
    let mut components = 0;
    for skill in &mut skills {
        effects::normalize_skill_effects(skill);
        for effect in &skill.effects {
            for component in &effect.components {
                components += 1;
                if let db::SkillEffectComponent::Unknown { text } = component {
                    *unknown_effects
                        .entry(format!("{}: {text}", effect.effect_type))
                        .or_default() += 1;
                }
            }
        }
    }
    let report = serde_json::json!({"items": item_reports, "skills": {"records": skills.len(), "components": components, "unknown_effects": unknown_effects}});
    pending.push(("data/skills.json", serde_json::to_vec_pretty(&skills)?));
    pending.push((
        "data/normalization-report.json",
        serde_json::to_vec_pretty(&report)?,
    ));
    for (path, bytes) in pending {
        atomic_write(std::path::Path::new(path), &bytes)?;
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn collect_items() -> Result<()> {
    let slugs = scraper::fetch_item_slugs()?;
    let (items, report) = collect_records(
        "items",
        &slugs,
        "data/items.partial.jsonl",
        scraper::fetch_item_for_bulk,
        |item: &Item| item.slug.clone(),
        |item: &Item| item.unparsed_tooltip_lines.clone(),
    )?;

    finish_collection(
        &items,
        &slugs,
        report,
        "data/items.json",
        "data/items.partial.jsonl",
    )
}

fn collect_set_piece_items() -> Result<()> {
    let item_sets: Vec<ItemSet> = serde_json::from_str(&fs::read_to_string("data/item-sets.json")?)
        .context("Could not read data/item-sets.json; run `fo2-dps item-sets` first")?;
    let slugs: Vec<String> = item_sets
        .into_iter()
        .flat_map(|item_set| item_set.pieces)
        .map(|piece| piece.slug)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let (items, report) = collect_records(
        "set-piece items",
        &slugs,
        "data/item-audit.partial.jsonl",
        scraper::fetch_item_for_bulk,
        |item: &Item| item.slug.clone(),
        |item: &Item| item.unparsed_tooltip_lines.clone(),
    )?;

    finish_collection(
        &items,
        &slugs,
        report,
        "data/item-audit.json",
        "data/item-audit.partial.jsonl",
    )
}

fn collect_mobs() -> Result<()> {
    let slugs = mobs::fetch_mob_slugs()?;
    let (captures, report) = collect_records(
        "mobs",
        &slugs,
        "data/mobs.partial.jsonl",
        mobs::fetch_mob_capture,
        |capture: &mobs::MobCapture| capture.mob.slug.clone(),
        |_| Vec::new(),
    )?;
    finish_mob_collection(&captures, &slugs, report)
}

fn finish_mob_collection(
    captures: &[mobs::MobCapture],
    slugs: &[String],
    report: ScrapeReport,
) -> Result<()> {
    const NORMALIZED_PATH: &str = "data/mobs.json";
    const ARCHIVE_PATH: &str = "data/mobs.raw.jsonl.gz";
    const MANIFEST_PATH: &str = "data/mobs.provenance.json";
    const CHECKPOINT_PATH: &str = "data/mobs.partial.jsonl";

    let requested = slugs.iter().collect::<BTreeSet<_>>().len();
    let report_path = "data/mobs.scrape-report.json";
    atomic_write(
        std::path::Path::new(report_path),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    if requested == 0 {
        bail!("No mob slugs discovered; refusing to publish an empty collection");
    }
    if !report.failures.is_empty() || captures.len() != requested {
        bail!(
            "Incomplete collection: saved {} mobs to {CHECKPOINT_PATH}; {} failures remain. Report: {report_path}",
            captures.len(),
            report.failures.len()
        );
    }

    let normalized = captures
        .iter()
        .map(|capture| &capture.mob)
        .collect::<Vec<_>>();
    let normalized_bytes = serde_json::to_vec_pretty(&normalized)?;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    for capture in captures {
        serde_json::to_writer(&mut encoder, &capture.archive)?;
        encoder.write_all(b"\n")?;
    }
    let archive_bytes = encoder.finish()?;

    let manifest = MobProvenance {
        schema_version: 1,
        record_count: captures.len(),
        normalized_path: NORMALIZED_PATH,
        normalized_sha256: sha256_hex(&normalized_bytes),
        archive_path: ARCHIVE_PATH,
        archive_format: "jsonl+gzip",
        archive_sha256: sha256_hex(&archive_bytes),
    };

    atomic_write(std::path::Path::new(ARCHIVE_PATH), &archive_bytes)?;
    atomic_write(std::path::Path::new(NORMALIZED_PATH), &normalized_bytes)?;
    atomic_write(
        std::path::Path::new(MANIFEST_PATH),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::remove_file(CHECKPOINT_PATH)
        .with_context(|| format!("Could not remove completed checkpoint {CHECKPOINT_PATH}"))?;

    println!(
        "Saved {} normalized mobs to {NORMALIZED_PATH} ({:.2} MiB), raw archive to {ARCHIVE_PATH} ({:.2} MiB), and provenance to {MANIFEST_PATH}. Report: {report_path}",
        captures.len(),
        normalized_bytes.len() as f64 / 1_048_576.0,
        archive_bytes.len() as f64 / 1_048_576.0,
    );
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn collect_skills() -> Result<()> {
    let slugs = scraper::fetch_skill_slugs()?;
    let (skills, report) = collect_records(
        "skills",
        &slugs,
        "data/skills.partial.jsonl",
        scraper::fetch_skill_for_bulk,
        |skill: &Skill| skill.slug.clone(),
        |_| Vec::new(),
    )?;

    finish_collection(
        &skills,
        &slugs,
        report,
        "data/skills.json",
        "data/skills.partial.jsonl",
    )
}

fn collect_records<T, Fetch, Slug, Unparsed>(
    entity: &str,
    slugs: &[String],
    checkpoint_path: &str,
    fetch: Fetch,
    slug_of: Slug,
    unparsed_lines_of: Unparsed,
) -> Result<(Vec<T>, ScrapeReport)>
where
    T: DeserializeOwned + Serialize,
    Fetch: Fn(&str) -> Result<T>,
    Slug: Fn(&T) -> String,
    Unparsed: Fn(&T) -> Vec<String>,
{
    let requested: BTreeSet<_> = slugs.iter().collect();
    if requested.is_empty() {
        bail!("No {entity} discovered; refusing an empty collection");
    }
    let mut completed = BTreeSet::new();
    let mut records: Vec<T> = read_jsonl(checkpoint_path)?;
    records.retain(|record| {
        let slug = slug_of(record);
        requested.contains(&slug) && completed.insert(slug)
    });
    let resumed = records.len();
    let mut failures = Vec::new();
    let mut scraped = 0;

    for slug in &requested {
        if completed.contains(*slug) {
            continue;
        }

        match fetch(slug).and_then(|record| {
            if slug_of(&record) != **slug {
                bail!("Fetched record slug does not match requested slug {slug}");
            }
            Ok(record)
        }) {
            Ok(record) => {
                append_jsonl(checkpoint_path, &record)?;
                completed.insert(slug_of(&record));
                records.push(record);
                scraped += 1;
            }
            Err(error) => {
                eprintln!("Failed to scrape {entity} {slug}: {error:#}");
                failures.push(ScrapeFailure {
                    slug: (*slug).clone(),
                    error: format!("{error:#}"),
                });
            }
        }

        std::thread::sleep(BULK_REQUEST_DELAY);
    }

    let mut unparsed_item_tooltip_lines = BTreeMap::new();
    for record in &records {
        for line in unparsed_lines_of(record) {
            *unparsed_item_tooltip_lines.entry(line).or_insert(0) += 1;
        }
    }

    Ok((
        records,
        ScrapeReport {
            entity: entity.to_string(),
            discovered: requested.len(),
            resumed,
            scraped,
            failures,
            unparsed_item_tooltip_lines,
        },
    ))
}

fn finish_collection<T: Serialize>(
    records: &[T],
    slugs: &[String],
    report: ScrapeReport,
    output_path: &str,
    checkpoint_path: &str,
) -> Result<()> {
    let requested = slugs.iter().collect::<BTreeSet<_>>().len();
    if requested == 0 {
        bail!("No slugs discovered; refusing to publish an empty collection");
    }
    let report_path = std::path::Path::new(output_path).with_extension("scrape-report.json");
    atomic_write(&report_path, &serde_json::to_vec_pretty(&report)?)?;
    let report_path = report_path.display();

    if report.failures.is_empty() && records.len() == requested {
        atomic_write(
            std::path::Path::new(output_path),
            &serde_json::to_vec_pretty(records)?,
        )?;
        fs::remove_file(checkpoint_path)
            .with_context(|| format!("Could not remove completed checkpoint {checkpoint_path}"))?;
        println!(
            "Saved {} {} to {output_path}. Report: {report_path}",
            records.len(),
            report.entity
        );
    } else {
        bail!(
            "Incomplete collection: saved {} {} to {checkpoint_path}; {} failures remain. Report: {report_path}",
            records.len(),
            report.entity,
            report.failures.len()
        );
    }

    Ok(())
}

fn read_jsonl<T: DeserializeOwned>(path: &str) -> Result<Vec<T>> {
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut records = Vec::new();
    let mut offset = 0;
    for (index, line) in contents.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let terminated = line.ends_with(b"\n");
        if !line.iter().all(u8::is_ascii_whitespace) {
            match serde_json::from_slice(line) {
                Ok(record) => records.push(record),
                Err(error) if !terminated && error.is_eof() => {
                    // Only an unterminated, incomplete tail can be discarded.
                    atomic_write(std::path::Path::new(path), &contents[..offset])?;
                    return Ok(records);
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("Invalid JSON checkpoint record at {path}:{}", index + 1)
                    });
                }
            }
        }
        offset += line.len();
    }
    if !contents.is_empty() && !contents.ends_with(b"\n") {
        let mut repaired = contents;
        repaired.push(b'\n');
        atomic_write(std::path::Path::new(path), &repaired)?;
    }
    Ok(records)
}

fn atomic_write(path: &std::path::Path, contents: &[u8]) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let (temporary, mut file) = loop {
        let temporary = parent.join(format!(
            ".fo2-dps-{}-{}.tmp",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let result = (|| -> Result<()> {
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("Could not atomically write {}", path.display()))
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "fo2-dps-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("{e}"),
                }
            }
        }
        fn path(&self, name: &str) -> String {
            self.0.join(name).to_str().unwrap().to_owned()
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn collect(slugs: &[String], checkpoint: &str) -> Result<(Vec<String>, ScrapeReport)> {
        collect_records(
            "test",
            slugs,
            checkpoint,
            |slug| Ok(slug.to_owned()),
            Clone::clone,
            |_| Vec::new(),
        )
    }

    #[test]
    fn resume_filters_and_deduplicates_requested_slugs() -> Result<()> {
        let dir = TempDir::new();
        let checkpoint = dir.path("partial.jsonl");
        fs::write(&checkpoint, b"\"a\"\n\"old\"\n\"a\"\n")?;
        let slugs = vec!["a".into(), "b".into(), "b".into()];
        let (records, report) = collect(&slugs, &checkpoint)?;
        assert_eq!(records, ["a", "b"]);
        assert_eq!(
            (report.discovered, report.resumed, report.scraped),
            (2, 1, 1)
        );
        let (_, report) = collect(&slugs, &checkpoint)?;
        assert_eq!((report.resumed, report.scraped), (2, 0));
        Ok(())
    }

    #[test]
    fn repairs_truncated_tail_and_separates_valid_unterminated_record() -> Result<()> {
        let dir = TempDir::new();
        let path = dir.path("partial.jsonl");
        for tail in ["\"truncated", "\"b\""] {
            fs::write(&path, format!("\"a\"\n{tail}"))?;
            let records: Vec<String> = read_jsonl(&path)?;
            assert_eq!(records.len(), if tail == "\"b\"" { 2 } else { 1 });
            append_jsonl(&path, &"c")?;
            let records: Vec<String> = read_jsonl(&path)?;
            assert_eq!(records.last().unwrap(), "c");
        }
        Ok(())
    }

    #[test]
    fn malformed_records_are_not_hidden_or_modified() -> Result<()> {
        let dir = TempDir::new();
        let path = dir.path("partial.jsonl");
        for bytes in [
            "\"a\"\n\"truncated\n",
            "invalid\n\"tail",
            "\"a\"\ninvalid",
            "123",
        ] {
            fs::write(&path, bytes)?;
            assert!(read_jsonl::<String>(&path).is_err());
            assert_eq!(fs::read_to_string(&path)?, bytes);
        }
        Ok(())
    }

    #[test]
    fn empty_discovery_preserves_files() -> Result<()> {
        let dir = TempDir::new();
        let path = dir.path("partial.jsonl");
        fs::write(&path, "untouched")?;
        assert!(collect(&[], &path).is_err());
        assert_eq!(fs::read_to_string(path)?, "untouched");
        Ok(())
    }

    #[test]
    fn publishes_complete_collection_and_removes_checkpoint() -> Result<()> {
        let dir = TempDir::new();
        let checkpoint = dir.path("partial.jsonl");
        let output = dir.path("final.json");
        fs::write(&output, "old final")?;
        let slugs = vec!["a".into(), "a".into()];
        let (records, report) = collect(&slugs, &checkpoint)?;
        finish_collection(&records, &slugs, report, &output, &checkpoint)?;
        assert_eq!(
            serde_json::from_slice::<Vec<String>>(&fs::read(output)?)?,
            ["a"]
        );
        assert!(!std::path::Path::new(&checkpoint).exists());
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path("final.scrape-report.json"))?)?;
        assert_eq!(report["discovered"], 1);
        assert_eq!(fs::read_dir(&dir.0)?.count(), 2);
        Ok(())
    }

    #[test]
    fn incomplete_collection_reports_error_and_preserves_final_and_checkpoint() -> Result<()> {
        let dir = TempDir::new();
        let checkpoint = dir.path("partial.jsonl");
        let output = dir.path("final.json");
        fs::write(&checkpoint, "\"a\"\n")?;
        fs::write(&output, "old final")?;
        let slugs = vec!["a".into(), "b".into()];
        let (records, report) = collect_records(
            "test",
            &slugs,
            &checkpoint,
            |_| -> Result<String> { bail!("offline failure") },
            Clone::clone,
            |_| Vec::new(),
        )?;
        assert!(finish_collection(&records, &slugs, report, &output, &checkpoint).is_err());
        assert_eq!(fs::read_to_string(output)?, "old final");
        assert_eq!(fs::read_to_string(checkpoint)?, "\"a\"\n");
        assert!(std::path::Path::new(&dir.path("final.scrape-report.json")).exists());
        Ok(())
    }

    #[test]
    fn failed_publication_keeps_checkpoint_and_cleans_temporary_file() -> Result<()> {
        let dir = TempDir::new();
        let checkpoint = dir.path("partial.jsonl");
        let output = dir.path("final.json");
        fs::create_dir(&output)?;
        let slugs = vec!["a".into()];
        let (records, report) = collect(&slugs, &checkpoint)?;
        assert!(finish_collection(&records, &slugs, report, &output, &checkpoint).is_err());
        assert!(std::path::Path::new(&checkpoint).exists());
        assert_eq!(fs::read_dir(&dir.0)?.count(), 3);
        Ok(())
    }
}

fn append_jsonl<T: Serialize>(path: &str, value: &T) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut record = serde_json::to_vec(value)?;
    record.push(b'\n');
    file.write_all(&record)?;
    file.sync_all()?;

    Ok(())
}
