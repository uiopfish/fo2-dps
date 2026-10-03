//! Read-only dataset validation. Wire `validate_data()` into the CLI when ready.
//!
//! Paths in findings are JSON pointers prefixed by the input filename. Indices,
//! not slugs, identify records so malformed and duplicate records remain visible.
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const INPUTS: [(&str, &str); 4] = [
    ("items.json", "item"),
    ("skills.json", "skill"),
    ("item-sets.json", "set"),
    ("mobs.json", "mob"),
];

/// Reads the four current datasets and optional `items.legacy.json` relative to
/// the working directory. Always writes the report before returning confirmed
/// validation errors; warnings alone succeed. I/O failures writing the report
/// are returned directly. No input data or raw evidence is rewritten.
pub fn validate_data() -> Result<()> {
    validate_directory(Path::new("data"))
}

#[derive(Default, Serialize)]
struct Report {
    schema_version: u32,
    summary: Summary,
    datasets: BTreeMap<String, Value>,
    /// Counts are by schema/field, including absent fields (not just present keys).
    field_states: BTreeMap<String, BTreeMap<String, usize>>,
    issues: Vec<Issue>,
    legacy_comparison: Value,
    limitations: Vec<&'static str>,
}

#[derive(Default, Serialize)]
struct Summary {
    errors: usize,
    warnings: usize,
    records: usize,
    links_checked: usize,
}

#[derive(Serialize)]
struct Issue {
    severity: &'static str,
    code: &'static str,
    path: String,
    message: String,
    evidence: Value,
}

impl Report {
    fn issue(
        &mut self,
        error: bool,
        code: &'static str,
        path: &str,
        message: impl Into<String>,
        evidence: Value,
    ) {
        if error {
            self.summary.errors += 1;
        } else {
            self.summary.warnings += 1;
        }
        self.issues.push(Issue {
            severity: if error { "error" } else { "warning" },
            code,
            path: path.into(),
            message: message.into(),
            evidence,
        });
    }

    fn state(&mut self, field: &str, state: &str) {
        *self
            .field_states
            .entry(field.into())
            .or_default()
            .entry(state.into())
            .or_default() += 1;
    }
}

fn load(path: &Path, optional: bool, report: &mut Report) -> Option<Value> {
    let result = fs::read(path)
        .map_err(anyhow::Error::from)
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(anyhow::Error::from));
    match result {
        Ok(value) if value.is_array() => Some(value),
        Ok(value) => {
            report.issue(
                !optional,
                "invalid_dataset",
                &path.display().to_string(),
                "Expected a JSON array; no records were validated from this input",
                value,
            );
            None
        }
        Err(error) => {
            report.issue(
                !optional,
                "unavailable_dataset",
                &path.display().to_string(),
                error.to_string(),
                Value::Null,
            );
            None
        }
    }
}

fn validate_directory(directory: &Path) -> Result<()> {
    let mut report = Report {
        schema_version: 3,
        limitations: vec![
            "Null means unknown or not applicable, never numeric zero; only explicit source states establish no cooldown/no timed duration/does not attack.",
            "Zone checks compare published identifiers internally; no authoritative zones dataset is available.",
            "Rolls are independent ordered entries: repeated rolls are allowed, percentages are not summed, and displayed rounded probabilities are not recomputed.",
            "Normalized mob records preserve selected published text and links; source HTML is archived separately and is not validated here.",
            "Name-only legacy matching cannot establish identity or prove an item was removed.",
        ],
        ..Report::default()
    };
    let mut data = BTreeMap::new();
    for (file, _) in INPUTS {
        let value = load(&directory.join(file), false, &mut report);
        report.datasets.insert(file.into(), json!({"loaded": value.is_some(), "records": value.as_ref().and_then(Value::as_array).map(Vec::len)}));
        if let Some(value) = value {
            data.insert(file.to_owned(), value);
        }
    }
    validate_current(&data, &mut report);
    let legacy = load(&directory.join("items.legacy.json"), true, &mut report);
    compare_legacy(legacy.as_ref(), data.get("items.json"), &mut report);
    let destination = directory.join("validation-report.json");
    atomic_write(&destination, &serde_json::to_vec_pretty(&report)?)?;
    println!(
        "Validation: {} records, {} links, {} errors, {} warnings. Report: {}",
        report.summary.records,
        report.summary.links_checked,
        report.summary.errors,
        report.summary.warnings,
        destination.display()
    );
    if report.summary.errors > 0 {
        bail!(
            "Data validation found {} confirmed errors (report written)",
            report.summary.errors
        );
    }
    Ok(())
}

// Exclusive same-directory temporary files avoid clobbering concurrent writers.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Report path needs a parent directory")?;
    for attempt in 0..100 {
        let temporary = parent.join(format!(
            ".validation-report.{}.{}.tmp",
            std::process::id(),
            attempt
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).context("Creating temporary validation report"),
        };
        let result = (|| -> Result<()> {
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result.with_context(|| format!("Writing {}", path.display()));
    }
    bail!("Could not reserve a temporary validation report filename")
}

// A small schema vocabulary mirrors db.rs and mobs.rs, not inferred game rules.
// `?` allows null and absent Option/default fields; missing is still reported.
fn fields(schema: &str) -> Option<&'static str> {
    Some(match schema {
        "item" => {
            "name:string slug:slug item_type:string description:?string image_url:?string implant_slot:?implant_slot damage_min:?u32 damage_max:?u32 attack_speed:?time stats:stats requirements:requirements market_price:?u64 recently_sold_price:?u64 shop_price:?u64 sellable_to_shops:bool rebirth:bool ascension:default_bool unparsed_tooltip_lines:[string]"
        }
        "stats" => {
            "armor:?i32 stamina:?i32 strength:?i32 agility:?i32 intellect:?i32 attack_power:?i32 crit:?i32 damage:?i32 cast_time_reduction:?i32 max_health:?i32 max_energy:?i32 health_regen:?i32 energy_regen:?i32"
        }
        "requirements" => {
            "level:?u32 stamina:?u32 strength:?u32 agility:?u32 intellect:?u32 faction_notoriety:?u32 guild_level:?u32"
        }
        "set" => {
            "name:string slug:slug bonus_rule:string resource_tool_rule:string pieces:[piece] bonuses:[bonus]"
        }
        "piece" => "name:string slug:slug item_type:string level_requirement:?u32",
        "bonus" => "required_pieces:positive_u32 effects:[bonus_effect]",
        "bonus_effect" => "stat:string value:string",
        "skill" => {
            "name:string slug:slug rank:?u32 quick_facts:[quick_fact] level_requirement:?u32 attribute_requirements:[attribute] cast_time:?time duration:?duration cooldown:?cooldown energy_cost:?u32 effects:[effect]"
        }
        "quick_fact" => "label:string value:string",
        "attribute" => "attribute:string amount:?u32 value:string",
        "effect" => "effect_type:string details:string components:default_components",
        "amount_body" => "amount:amount",
        "modifier" => "stat:string value:i32",
        "periodic" => {
            "resource:resource direction:direction amount:amount interval_seconds:positive_u32"
        }
        "threat" => "multiplier:u32",
        "unknown" => "text:string",
        "range32" => "min:u32 max:u32",
        "range64" => "min:u64 max:u64",
        "mob" => {
            "slug:slug name:string source_url:string level:?u64 health:?u64 damage:?range64 attack_speed_ms:?u64 attacks:?bool faction:?mob_value faction_xp:?i64 required_weapon:?mob_value aggressive:?bool debuff_skill_count:?u64 debuffs:[mob_value] locations:[location] drop_profiles:[profile] boss_candidate:bool"
        }
        "link" => "href:string text:string aria_label:?string",
        "mob_value" => "text:string links:[link]",
        "location" => "zone:link map_location_count:?u64",
        "profile" => {
            "summary:mob_value zone:?mob_value map_location_count:?u64 solo_coins:?range64 drops:[drop] notice:?string"
        }
        "drop" => {
            "item:mob_value rolls:[roll] solo_chance_at_least_one_percent:?percent maximum_quantity:?u64"
        }
        "roll" => "source:?string chance_percent:?percent",
        _ => return None,
    })
}

fn pointer(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn check(value: &Value, kind: &str, path: &str, report: &mut Report) {
    if let Some(inner) = kind.strip_prefix('?') {
        if !value.is_null() {
            check(value, inner, path, report);
        }
        return;
    }
    if kind == "default_bool" {
        check(value, "bool", path, report);
        return;
    }
    if kind == "default_components" {
        check(value, "[component]", path, report);
        return;
    }
    if let Some(inner) = kind.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if let Some(array) = value.as_array() {
            for (i, v) in array.iter().enumerate() {
                check(v, inner, &pointer(path, &i.to_string()), report);
            }
        } else {
            malformed(report, path, kind, value);
        }
        return;
    }
    if let Some(spec) = fields(kind) {
        let Some(object) = value.as_object() else {
            malformed(report, path, kind, value);
            return;
        };
        let known: BTreeMap<_, _> = spec
            .split_whitespace()
            .map(|s| s.split_once(':').unwrap())
            .collect();
        for (key, ty) in &known {
            let field_path = pointer(path, key);
            let field = format!("{kind}.{key}");
            match object.get(*key) {
                None => {
                    report.state(&field, "missing");
                    report.issue(
                        !ty.starts_with('?') && !ty.starts_with("default_"),
                        "missing_field",
                        &field_path,
                        "Field is absent; no value has been inferred",
                        Value::Null,
                    );
                }
                Some(v) => {
                    report.state(&field, state(v));
                    check(v, ty, &field_path, report);
                }
            }
        }
        for (key, v) in object {
            if !known.contains_key(key.as_str()) {
                report.state(&format!("{kind}.{key}"), "unknown_field");
                report.issue(false, "unknown_field", &pointer(path, key), "Field is outside the inspected model; retained as evidence, not semantically validated", v.clone());
            }
        }
        if matches!(kind, "range32" | "range64") {
            range(value, "min", "max", path, report);
        }
        if kind == "item" {
            range(value, "damage_min", "damage_max", path, report);
            if value["attack_speed"].as_f64() == Some(0.0) {
                report.issue(
                    false,
                    "zero_attack_timing",
                    &pointer(path, "attack_speed"),
                    "Zero timing is explicit; units/game semantics need review",
                    json!(0),
                );
            }
        }
        if kind == "unknown" {
            report.issue(
                false,
                "unparsed_effect",
                path,
                "Effect is explicitly unparsed",
                value.clone(),
            );
        }
        if kind == "item"
            && value["unparsed_tooltip_lines"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
        {
            report.issue(
                false,
                "unparsed_tooltip",
                &pointer(path, "unparsed_tooltip_lines"),
                "Tooltip lines remain unparsed",
                value["unparsed_tooltip_lines"].clone(),
            );
        }
        return;
    }
    let valid = match kind {
        "string" => value.is_string(),
        "slug" => value.as_str().is_some_and(valid_slug),
        "bool" => value.is_boolean(),
        "u32" | "positive_u32" => value
            .as_u64()
            .is_some_and(|n| n <= u32::MAX as u64 && (kind != "positive_u32" || n > 0)),
        "u64" => value.as_u64().is_some(),
        "i32" => value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
        "i64" => value.as_i64().is_some(),
        "time" => value
            .as_f64()
            .is_some_and(|n| n.is_finite() && n >= 0.0 && n <= f32::MAX as f64),
        "percent" => value
            .as_f64()
            .is_some_and(|n| n.is_finite() && (0.0..=100.0).contains(&n)),
        "resource" => matches!(value.as_str(), Some("Health" | "Energy")),
        "direction" => matches!(value.as_str(), Some("Gain" | "Loss")),
        "implant_slot" => matches!(
            value.as_str(),
            Some("brain" | "heart" | "left-arm" | "right-arm" | "left-leg" | "right-leg")
        ),
        "duration" | "cooldown" | "amount" | "component" => {
            check_enum(value, kind, path, report);
            return;
        }
        _ => unreachable!("Unknown validator schema {kind}"),
    };
    if !valid {
        malformed(report, path, kind, value);
    }
}

fn state(value: &Value) -> &'static str {
    match value {
        Value::Null => "null_unknown_or_not_applicable",
        Value::Bool(false) => "explicit_false",
        Value::Bool(true) => "explicit_true",
        Value::Number(n) if n.as_f64() == Some(0.0) => "zero",
        Value::String(s) if matches!(s.as_str(), "None" | "NoTimedDuration") => {
            "explicit_no_timing"
        }
        Value::Array(a) if a.is_empty() => "empty_array",
        Value::String(s) if s.is_empty() => "empty_string",
        _ => "present",
    }
}

fn malformed(report: &mut Report, path: &str, kind: &str, value: &Value) {
    report.issue(
        true,
        if kind == "slug" {
            "invalid_slug"
        } else {
            "invalid_field"
        },
        path,
        format!("Expected {kind}, including its numeric bounds where applicable"),
        value.clone(),
    );
}

fn check_enum(value: &Value, kind: &str, path: &str, report: &mut Report) {
    let variants: &[(&str, Option<&str>)] = match kind {
        "duration" => &[("NoTimedDuration", None), ("Seconds", Some("time"))],
        "cooldown" => &[("None", None), ("Seconds", Some("time"))],
        "amount" => &[("Fixed", Some("u32")), ("Range", Some("range32"))],
        "component" => &[
            ("DirectDamage", Some("amount_body")),
            ("DirectHeal", Some("amount_body")),
            ("HealthLoss", Some("amount_body")),
            ("TimedStatModifier", Some("modifier")),
            ("Periodic", Some("periodic")),
            ("ThreatMultiplier", Some("threat")),
            ("Teleport", None),
            ("Pet", None),
            ("Morph", None),
            ("Unknown", Some("unknown")),
        ],
        _ => unreachable!(),
    };
    let (tag, body) = if let Some(tag) = value.as_str() {
        (tag, None)
    } else if let Some(obj) = value.as_object().filter(|o| o.len() == 1) {
        let (tag, body) = obj.iter().next().unwrap();
        (tag.as_str(), Some(body))
    } else {
        malformed(report, path, kind, value);
        return;
    };
    match variants.iter().find(|(name, _)| *name == tag) {
        Some((_, Some(ty))) if body.is_some() => {
            check(body.unwrap(), ty, &pointer(path, tag), report)
        }
        Some((_, None)) if body.is_none() => (),
        Some(_) => malformed(report, path, kind, value),
        None => report.issue(
            false,
            "unknown_variant",
            path,
            format!("Unrecognized {kind} variant; not semantically validated"),
            value.clone(),
        ),
    }
}

fn range(value: &Value, min: &str, max: &str, path: &str, report: &mut Report) {
    let a = value.get(min).and_then(Value::as_u64);
    let b = value.get(max).and_then(Value::as_u64);
    if let (Some(a), Some(b)) = (a, b) {
        if a > b {
            report.issue(
                true,
                "reversed_range",
                path,
                format!("{min} exceeds {max}"),
                json!({min: a, max: b}),
            );
        }
    } else if a.is_some() != b.is_some() {
        report.issue(
            false,
            "partial_range",
            path,
            "Only one numeric endpoint is known; no endpoint inferred",
            json!({min: value.get(min), max: value.get(max)}),
        );
    }
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}

type Index = BTreeMap<String, Vec<usize>>;
fn index(records: &Value) -> Index {
    let mut result = Index::new();
    for (i, record) in array(records).iter().enumerate() {
        if let Some(slug) = record["slug"].as_str().filter(|s| valid_slug(s)) {
            result.entry(slug.into()).or_default().push(i);
        }
    }
    result
}

fn validate_implant_slot(item: &Value, path: &str, report: &mut Report) {
    let expected = match item["description"].as_str() {
        Some("Implant for your Brain.") => Some("brain"),
        Some("Implant for your Heart.") => Some("heart"),
        Some("Implant for your Left Arm.") => Some("left-arm"),
        Some("Implant for your Right Arm.") => Some("right-arm"),
        Some("Implant for your Left Leg.") => Some("left-leg"),
        Some("Implant for your Right Leg.") => Some("right-leg"),
        _ => None,
    };
    let actual = item["implant_slot"].as_str();
    if item["item_type"].as_str() == Some("Implant") {
        if expected.is_none() {
            report.issue(
                true,
                "unrecognized_implant_description",
                &pointer(path, "description"),
                "Implant description does not identify one supported body-part slot",
                item["description"].clone(),
            );
        } else if actual != expected {
            report.issue(
                true,
                "incorrect_implant_slot",
                &pointer(path, "implant_slot"),
                "Typed implant slot must match the published item description",
                json!({"expected": expected, "actual": actual}),
            );
        }
    } else if actual.is_some() {
        report.issue(
            true,
            "implant_slot_on_non_implant",
            &pointer(path, "implant_slot"),
            "Only Implant items may have an implant slot",
            item["implant_slot"].clone(),
        );
    }
}

fn validate_current(data: &BTreeMap<String, Value>, report: &mut Report) {
    let indices: BTreeMap<_, _> = data
        .iter()
        .map(|(file, records)| (file.as_str(), index(records)))
        .collect();
    for (file, kind) in INPUTS {
        let Some(records) = data.get(file) else {
            continue;
        };
        report.summary.records += array(records).len();
        if array(records).is_empty() {
            report.issue(
                false,
                "empty_dataset",
                file,
                "Dataset contains no records",
                json!([]),
            );
        }
        for (i, record) in array(records).iter().enumerate() {
            let path = pointer(file, &i.to_string());
            check(record, kind, &path, report);
            if record["name"].as_str().is_some_and(|s| s.trim().is_empty()) {
                report.issue(
                    true,
                    "empty_name",
                    &pointer(&path, "name"),
                    "Record name must not be blank",
                    record["name"].clone(),
                );
            }
            if kind == "item" {
                validate_implant_slot(record, &path, report);
            }
        }
        for (slug, positions) in &indices[file] {
            if positions.len() > 1 {
                for position in positions {
                    report.issue(
                        true,
                        "duplicate_slug",
                        &format!("{file}/{position}/slug"),
                        "Slug identifies multiple records",
                        json!({"slug": slug, "record_indices": positions}),
                    );
                }
            }
        }
    }
    let items = indices.get("items.json");
    let skills = indices.get("skills.json");
    for (i, set) in array(data.get("item-sets.json").unwrap_or(&Value::Null))
        .iter()
        .enumerate()
    {
        for (j, piece) in array(&set["pieces"]).iter().enumerate() {
            if let Some(slug) = piece["slug"].as_str().filter(|s| valid_slug(s)) {
                resolve(
                    slug,
                    items,
                    &format!("item-sets.json/{i}/pieces/{j}/slug"),
                    "item",
                    report,
                );
            }
        }
        for (j, bonus) in array(&set["bonuses"]).iter().enumerate() {
            if bonus["required_pieces"]
                .as_u64()
                .is_some_and(|n| n > array(&set["pieces"]).len() as u64)
            {
                report.issue(false, "set_tier_exceeds_list", &format!("item-sets.json/{i}/bonuses/{j}"), "Tier exceeds listed pieces; list completeness and equipment rules are not known", bonus.clone());
            }
        }
    }
    let mut zone_names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut name_zones: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (i, mob) in array(data.get("mobs.json").unwrap_or(&Value::Null))
        .iter()
        .enumerate()
    {
        let path = format!("mobs.json/{i}");
        if let Some(url) = mob["source_url"].as_str() {
            if entity_slug(url, "mobs").as_deref() != mob["slug"].as_str() {
                report.issue(
                    true,
                    "source_slug_mismatch",
                    &pointer(&path, "source_url"),
                    "Source URL does not identify this mob slug",
                    json!({"url": url, "slug": mob["slug"]}),
                );
            }
        }
        if mob["attacks"] == true && mob["attack_speed_ms"].as_u64() == Some(0) {
            report.issue(
                true,
                "zero_attack_interval",
                &pointer(&path, "attack_speed_ms"),
                "An attacking mob has a zero attack interval",
                json!(0),
            );
        }
        if mob["attacks"] == false && mob["attack_speed_ms"].as_u64().is_some_and(|n| n > 0) {
            report.issue(
                false,
                "nonattacker_timing",
                &path,
                "Explicit nonattacker also has attack timing; review source",
                mob["attack_speed_ms"].clone(),
            );
        }
        if let (Some(count), Some(debuffs)) = (
            mob["debuff_skill_count"].as_u64(),
            mob["debuffs"].as_array(),
        ) {
            if count != debuffs.len() as u64 {
                report.issue(
                    false,
                    "debuff_count_mismatch",
                    &pointer(&path, "debuffs"),
                    "Published debuff count differs from listed entries",
                    json!({"published": count, "listed": debuffs.len()}),
                );
            }
        }
        for (j, debuff) in array(&mob["debuffs"]).iter().enumerate() {
            linked_value(
                debuff,
                "skills",
                skills,
                &format!("{path}/debuffs/{j}"),
                report,
            );
        }
        let mut locations = BTreeSet::new();
        for (j, location) in array(&mob["locations"]).iter().enumerate() {
            let p = format!("{path}/locations/{j}/zone");
            if let Some(slug) = zone_link(
                &location["zone"],
                &p,
                &mut zone_names,
                &mut name_zones,
                report,
            ) {
                locations.insert(slug);
            }
        }
        for (j, profile) in array(&mob["drop_profiles"]).iter().enumerate() {
            let p = format!("{path}/drop_profiles/{j}");
            if let Some(zone) = profile.get("zone").filter(|v| !v.is_null()) {
                let mut found = false;
                for (k, link) in array(&zone["links"]).iter().enumerate() {
                    let zpath = format!("{p}/zone/links/{k}");
                    if let Some(slug) =
                        zone_link(link, &zpath, &mut zone_names, &mut name_zones, report)
                    {
                        found = true;
                        if !locations.contains(&slug) {
                            report.issue(false, "profile_zone_not_in_locations", &zpath, "Profile zone is absent from this mob's location list; completeness is unknown", json!(slug));
                        }
                    }
                }
                if !found {
                    report.issue(
                        false,
                        "unidentified_zone",
                        &format!("{p}/zone"),
                        "No usable linked zone identifier",
                        zone.clone(),
                    );
                }
            }
            for (k, drop) in array(&profile["drops"]).iter().enumerate() {
                linked_value(
                    &drop["item"],
                    "items",
                    items,
                    &format!("{p}/drops/{k}/item"),
                    report,
                );
            }
        }
    }
    for (slug, names) in zone_names {
        if names.len() > 1 {
            report.issue(
                false,
                "zone_name_conflict",
                "mobs.json",
                "One zone identifier has multiple labels",
                json!({"slug": slug, "names": names}),
            );
        }
    }
    for (name, slugs) in name_zones {
        if slugs.len() > 1 {
            report.issue(
                false,
                "zone_identifier_conflict",
                "mobs.json",
                "One zone label has multiple identifiers; aliases cannot be inferred",
                json!({"name": name, "slugs": slugs}),
            );
        }
    }
}

// Only local paths or the actual database origin qualify. Query/fragment data
// is ignored for identity, never parsed as another roll or another reference.
fn entity_slug(href: &str, entity: &str) -> Option<String> {
    let path = href
        .strip_prefix("https://db.fantasyonline2.com")
        .unwrap_or(href);
    let path = path.split(['?', '#']).next()?;
    let slug = path
        .strip_prefix(&format!("/{entity}/"))?
        .trim_end_matches('/');
    valid_slug(slug).then(|| slug.to_owned())
}

fn resolve(slug: &str, target: Option<&Index>, path: &str, entity: &str, report: &mut Report) {
    report.summary.links_checked += 1;
    match target {
        None => report.issue(
            false,
            "reference_not_checked",
            path,
            format!("{entity} dataset unavailable"),
            json!(slug),
        ),
        Some(index) => match index.get(slug) {
            None => report.issue(
                true,
                "unresolved_reference",
                path,
                format!("No {entity} record has this slug"),
                json!(slug),
            ),
            Some(records) if records.len() > 1 => report.issue(
                true,
                "ambiguous_reference",
                path,
                format!("Multiple {entity} records have this slug"),
                json!({"slug": slug, "record_indices": records}),
            ),
            _ => (),
        },
    }
}

fn linked_value(
    value: &Value,
    entity: &str,
    target: Option<&Index>,
    path: &str,
    report: &mut Report,
) {
    let mut found = false;
    for (i, link) in array(&value["links"]).iter().enumerate() {
        if let Some(slug) = link["href"].as_str().and_then(|s| entity_slug(s, entity)) {
            found = true;
            resolve(
                &slug,
                target,
                &format!("{path}/links/{i}/href"),
                entity,
                report,
            );
        } else {
            report.issue(
                false,
                "unrecognized_reference_link",
                &format!("{path}/links/{i}"),
                format!("Not a recognized database {entity} URL"),
                link.clone(),
            );
        }
    }
    if !found {
        report.issue(
            false,
            "unlinked_reference",
            path,
            format!("No usable {entity} slug; name alone does not establish identity"),
            value.clone(),
        );
    }
}

fn zone_link(
    link: &Value,
    path: &str,
    names: &mut BTreeMap<String, BTreeSet<String>>,
    ids: &mut BTreeMap<String, BTreeSet<String>>,
    report: &mut Report,
) -> Option<String> {
    let slug = link["href"].as_str().and_then(|s| entity_slug(s, "zones"));
    if let Some(slug) = &slug {
        report.summary.links_checked += 1;
        if let Some(name) = link["text"].as_str().filter(|s| !s.trim().is_empty()) {
            let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
            names.entry(slug.clone()).or_default().insert(name.clone());
            ids.entry(name).or_default().insert(slug.clone());
        }
    } else {
        report.issue(
            false,
            "invalid_zone_identifier",
            path,
            "Not a recognized database zone URL",
            link.clone(),
        );
    }
    slug
}

fn name_index(records: &Value) -> BTreeMap<String, Vec<usize>> {
    let mut names = BTreeMap::<String, Vec<usize>>::new();
    for (i, record) in array(records).iter().enumerate() {
        if let Some(name) = record["name"].as_str().filter(|s| !s.trim().is_empty()) {
            names.entry(name.to_owned()).or_default().push(i);
        }
    }
    names
}

fn compare_legacy(legacy: Option<&Value>, current: Option<&Value>, report: &mut Report) {
    let (Some(legacy), Some(current)) = (legacy, current) else {
        report.legacy_comparison = json!({"status": "unavailable", "confirmed_missing": 0});
        return;
    };
    let old_names = name_index(legacy);
    let new_names = name_index(current);
    let mut entries = Vec::new();
    let mut counts = BTreeMap::<&str, usize>::new();
    for (i, record) in array(legacy).iter().enumerate() {
        let name = record["name"].as_str();
        let matches = name
            .and_then(|n| new_names.get(n))
            .cloned()
            .unwrap_or_default();
        let status = match name.filter(|s| !s.trim().is_empty()) {
            None => "invalid_legacy_name",
            Some(_) if matches.is_empty() => "unmatched_name_cannot_prove_missing",
            Some(n) if matches.len() > 1 || old_names[n].len() > 1 => "ambiguous_name",
            Some(_) => "unique_name_match_not_proven_identity",
        };
        *counts.entry(status).or_default() += 1;
        let entry =
            json!({"legacy_index": i, "name": name, "status": status, "current_indices": matches});
        if status != "unique_name_match_not_proven_identity" {
            report.issue(
                false,
                "legacy_name_comparison",
                &format!("items.legacy.json/{i}"),
                status,
                entry.clone(),
            );
        }
        entries.push(entry);
    }
    report.legacy_comparison = json!({"status": "compared_by_exact_name", "legacy_records": array(legacy).len(), "current_records": array(current).len(), "confirmed_missing": 0, "counts": counts, "entries": entries});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_fields_do_not_hide_siblings_or_records() {
        let mut report = Report::default();
        check(
            &json!({"min": "wrong", "max": -1, "extra": 7}),
            "range64",
            "mobs.json/0/damage",
            &mut report,
        );
        check(
            &json!([null, {"min": 3, "max": 2}]),
            "[range64]",
            "ranges",
            &mut report,
        );
        assert_eq!(report.summary.errors, 4);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.code == "unknown_field" && i.evidence == 7)
        );
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.path == "ranges/1" && i.code == "reversed_range")
        );
    }

    #[test]
    fn zero_null_missing_and_explicit_states_are_distinct() {
        let mut report = Report::default();
        for v in [
            json!({"chance_percent": 0}),
            json!({"chance_percent": null}),
            json!({}),
        ] {
            check(&v, "roll", "roll", &mut report);
        }
        let counts = &report.field_states["roll.chance_percent"];
        assert_eq!(counts["zero"], 1);
        assert_eq!(counts["null_unknown_or_not_applicable"], 1);
        assert_eq!(counts["missing"], 1);
        let mut timing = Report::default();
        for (v, ty) in [
            (json!(null), "?cooldown"),
            (json!("None"), "cooldown"),
            (json!({"Seconds": 0}), "cooldown"),
            (json!("NoTimedDuration"), "duration"),
        ] {
            check(&v, ty, "skill", &mut timing);
        }
        assert_eq!(timing.summary.errors, 0);
        assert_ne!(state(&json!(null)), state(&json!(0)));
        assert_ne!(state(&json!(false)), state(&json!(null)));
        assert_eq!(state(&json!("None")), "explicit_no_timing");
        check(&json!({"Seconds": -0.1}), "duration", "skill", &mut timing);
        check(&json!(0), "positive_u32", "interval", &mut timing);
        assert_eq!(timing.summary.errors, 2);
    }

    #[test]
    fn unresolved_links_and_repeated_rolls() {
        let roll = json!({"source": "Spawn", "chance_percent": 80});
        let drop = json!({"item": {"text": "Missing", "links": [{"href": "/items/missing-1?x=2#roll", "text": "Missing", "aria_label": null}]}, "rolls": [roll.clone(), roll], "solo_chance_at_least_one_percent": 96, "maximum_quantity": 2});
        let mut report = Report::default();
        check(&drop, "drop", "drop", &mut report);
        assert_eq!(report.summary.errors, 0);
        assert_eq!(report.summary.warnings, 0);
        linked_value(
            &drop["item"],
            "items",
            Some(&Index::new()),
            "drop/item",
            &mut report,
        );
        assert_eq!(report.summary.errors, 1);
        assert_eq!(report.issues[0].code, "unresolved_reference");
        check(&json!(100.01), "percent", "chance", &mut report);
        check(&json!(-1), "percent", "chance", &mut report);
        assert_eq!(report.summary.errors, 3);
    }

    #[test]
    fn normalized_mob_schema_accepts_boundaries_and_rejects_legacy_raw_fields() {
        let mob = json!({
            "slug": "boundary-mob",
            "name": "Boundary Mob",
            "source_url": "/mobs/boundary-mob",
            "level": 0,
            "health": null,
            "damage": {"min": 0, "max": 0},
            "attack_speed_ms": null,
            "attacks": false,
            "faction": {"text": "None", "links": []},
            "faction_xp": 0,
            "required_weapon": null,
            "aggressive": false,
            "debuff_skill_count": 0,
            "debuffs": [],
            "locations": [{
                "zone": {"href": "/zones/boundary-zone", "text": "Boundary Zone", "aria_label": null},
                "map_location_count": 0
            }],
            "drop_profiles": [{
                "summary": {"text": "No drops", "links": []},
                "zone": null,
                "map_location_count": 0,
                "solo_coins": {"min": 0, "max": 0},
                "drops": [{
                    "item": {"text": "Boundary Item", "links": []},
                    "rolls": [
                        {"source": null, "chance_percent": 0},
                        {"source": null, "chance_percent": 0}
                    ],
                    "solo_chance_at_least_one_percent": 0,
                    "maximum_quantity": 0
                }],
                "notice": null
            }],
            "boss_candidate": false
        });
        let mut report = Report::default();
        check(&mob, "mob", "mobs.json/0", &mut report);
        assert_eq!(report.summary.errors, 0);
        assert_eq!(report.summary.warnings, 0);
        assert_eq!(
            report.field_states["mob.boss_candidate"]["explicit_false"],
            1
        );
        assert_eq!(
            report.field_states["profile.notice"]["null_unknown_or_not_applicable"],
            1
        );

        let mut missing_boss = mob.clone();
        missing_boss
            .as_object_mut()
            .unwrap()
            .remove("boss_candidate");
        check(&missing_boss, "mob", "mobs.json/1", &mut report);
        assert!(report.issues.iter().any(|issue| {
            issue.code == "missing_field" && issue.path == "mobs.json/1/boss_candidate"
        }));

        for (kind, value, legacy_field) in [
            (
                "mob_value",
                json!({"text": "x", "links": [], "html": "x"}),
                "html",
            ),
            (
                "location",
                json!({
                    "zone": {"href": "/zones/x", "text": "X", "aria_label": null},
                    "map_location_count": null,
                    "raw": {"text": "x", "links": []}
                }),
                "raw",
            ),
            (
                "roll",
                json!({
                    "source": null,
                    "chance_percent": null,
                    "raw": {"text": "x", "links": []}
                }),
                "raw",
            ),
        ] {
            let start = report.issues.len();
            check(&value, kind, kind, &mut report);
            assert!(report.issues[start..].iter().any(|issue| {
                issue.code == "unknown_field" && issue.path == format!("{kind}/{legacy_field}")
            }));
        }
        assert!(fields("fact").is_none());
        assert!(fields("table").is_none());
        assert!(fields("section").is_none());
        assert!(fields("raw_page").is_none());
    }

    #[test]
    fn duplicate_indices_and_malformed_records_are_preserved() {
        let records = json!([{"slug": "same"}, null, {"slug": "same"}, {"slug": ""}]);
        assert_eq!(index(&records)["same"], vec![0, 2]);
        let mut report = Report::default();
        validate_current(
            &BTreeMap::from([("items.json".into(), records)]),
            &mut report,
        );
        assert_eq!(report.summary.records, 4);
        assert_eq!(
            report
                .issues
                .iter()
                .filter(|i| i.code == "duplicate_slug")
                .count(),
            2
        );
        assert!(report.issues.iter().any(|i| i.path == "items.json/1"));
        assert!(report.issues.iter().any(|i| i.code == "invalid_slug"));
    }

    #[test]
    fn legacy_names_never_prove_removal() {
        let mut report = Report::default();
        compare_legacy(
            Some(&json!([{"name": "A"}, {"name": "A"}, {"name": "Gone"}, null, {"name": "B"}])),
            Some(&json!([{"name": "A"}, {"name": "B"}])),
            &mut report,
        );
        assert_eq!(
            report.legacy_comparison["entries"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert_eq!(report.legacy_comparison["counts"]["ambiguous_name"], 2);
        assert_eq!(report.legacy_comparison["confirmed_missing"], 0);
        assert_eq!(report.summary.errors, 0);
    }

    #[test]
    fn zone_identity_and_reference_resolution_are_conservative() {
        assert_eq!(
            entity_slug("https://db.fantasyonline2.com/zones/a-1/?x=2#p", "zones"),
            Some("a-1".into())
        );
        assert_eq!(entity_slug("https://evil.example/zones/a-1", "zones"), None);
        assert_eq!(entity_slug("/zones/a/b", "zones"), None);
        let mut report = Report::default();
        let mut names = BTreeMap::new();
        let mut ids = BTreeMap::new();
        zone_link(
            &json!({"href": "/zones/a-1", "text": "A"}),
            "zone",
            &mut names,
            &mut ids,
            &mut report,
        );
        zone_link(
            &json!({"href": "/zones/a-2", "text": "A"}),
            "zone",
            &mut names,
            &mut ids,
            &mut report,
        );
        assert_eq!(ids["A"].len(), 2);
        resolve("missing", None, "ref", "item", &mut report);
        assert_eq!(report.summary.errors, 0);
        assert_eq!(report.issues[0].code, "reference_not_checked");
    }

    #[test]
    fn report_written_before_error_and_warnings_succeed() {
        let directory =
            std::env::temp_dir().join(format!("fo2-validation-test-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let result = (|| -> Result<()> {
            for (file, _) in INPUTS {
                fs::write(directory.join(file), b"[]")?;
            }
            validate_directory(&directory)?;
            let warnings: Value =
                serde_json::from_slice(&fs::read(directory.join("validation-report.json"))?)?;
            assert_eq!(warnings["schema_version"], 3);
            assert_eq!(warnings["summary"]["errors"], 0);
            assert!(warnings["summary"]["warnings"].as_u64().unwrap() > 0);
            fs::write(directory.join("items.json"), b"[null, {}, 123]")?;
            fs::write(directory.join("skills.json"), b"malformed JSON")?;
            assert!(validate_directory(&directory).is_err());
            let errors: Value =
                serde_json::from_slice(&fs::read(directory.join("validation-report.json"))?)?;
            assert_eq!(errors["summary"]["records"], 3);
            assert!(errors["summary"]["errors"].as_u64().unwrap() > 0);
            assert_eq!(fs::read(directory.join("items.json"))?, b"[null, {}, 123]");
            Ok(())
        })();
        fs::remove_dir_all(directory).unwrap();
        result.unwrap();
    }
}
