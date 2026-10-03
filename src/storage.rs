//! Build the derived SQLite query database from the checked-in JSON datasets.

use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, Transaction, params};
use serde::de::DeserializeOwned;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::{
    EffectAmount, EffectDirection, EffectResource, Item, ItemSet, Skill, SkillCooldown,
    SkillDuration, SkillEffectComponent,
};
use crate::mobs::{Mob, MobValue};

const SCHEMA_VERSION: i64 = 4;

/// Rebuild `data/fo2.sqlite` from the four authoritative JSON files.
///
/// The database is first completed and verified in a unique sibling file. The
/// existing database is replaced only after every import and check succeeds.
pub fn build_database() -> Result<()> {
    build_database_in(Path::new("data"))
}

fn build_database_in(data_dir: &Path) -> Result<()> {
    let items = read_dataset::<Item>(data_dir, "items.json")?;
    let skills = read_dataset::<Skill>(data_dir, "skills.json")?;
    let sets = read_dataset::<ItemSet>(data_dir, "item-sets.json")?;
    let mobs = read_dataset::<Mob>(data_dir, "mobs.json")?;
    let destination = data_dir.join("fo2.sqlite");
    let temporary = unique_temp_path(data_dir)?;

    let result = (|| {
        let mut connection = Connection::open(&temporary)
            .with_context(|| format!("opening temporary database {}", temporary.display()))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        import_all(&mut connection, data_dir, &items, &skills, &sets, &mobs)?;
        drop(connection);
        fs::rename(&temporary, &destination).with_context(|| {
            format!(
                "publishing {} as {}",
                temporary.display(),
                destination.display()
            )
        })?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_dataset<T: DeserializeOwned>(data_dir: &Path, name: &str) -> Result<Dataset<T>> {
    let path = data_dir.join(name);
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let records = serde_json::from_slice::<Vec<T>>(&bytes)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(Dataset {
        source_path: format!("data/{name}"),
        file_bytes: to_i64(bytes.len(), "source file byte count")?,
        records,
    })
}

struct Dataset<T> {
    source_path: String,
    file_bytes: i64,
    records: Vec<T>,
}

fn unique_temp_path(dir: &Path) -> Result<PathBuf> {
    let pid = std::process::id();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates Unix epoch")?
        .as_nanos();
    for attempt in 0..100u32 {
        let path = dir.join(format!(".fo2.sqlite.{pid}.{nonce}.{attempt}.tmp"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                drop(file);
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("creating {}", path.display()));
            }
        }
    }
    bail!(
        "could not allocate a unique temporary database in {}",
        dir.display()
    )
}

fn import_all(
    connection: &mut Connection,
    data_dir: &Path,
    items: &Dataset<Item>,
    skills: &Dataset<Skill>,
    sets: &Dataset<ItemSet>,
    mobs: &Dataset<Mob>,
) -> Result<()> {
    let transaction = connection.transaction()?;
    create_schema(&transaction)?;
    let run_id = transaction.query_row(
        "INSERT INTO import_runs(schema_version, imported_at) VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ','now')) RETURNING id",
        [SCHEMA_VERSION],
        |row| row.get::<_, i64>(0),
    )?;
    import_items(&transaction, &items.records)?;
    import_skills(&transaction, &skills.records)?;
    import_sets(&transaction, &sets.records)?;
    import_mobs(&transaction, &mobs.records)?;

    for (name, dataset) in [
        ("items", dataset_meta(items)?),
        ("skills", dataset_meta(skills)?),
        ("item_sets", dataset_meta(sets)?),
        ("mobs", dataset_meta(mobs)?),
    ] {
        transaction.execute(
            "INSERT INTO source_datasets(import_run_id, dataset_name, source_path, record_count, file_bytes, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            params![run_id, name, dataset.0, dataset.1, dataset.2],
        )?;
    }

    validate_counts(&transaction, items, skills, sets, mobs)?;
    let fk_errors: i64 =
        transaction.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    ensure!(
        fk_errors == 0,
        "foreign_key_check reported {fk_errors} violation(s)"
    );
    let integrity: String =
        transaction.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    ensure!(integrity == "ok", "integrity_check failed: {integrity}");
    transaction.commit()?;

    // Keep this parameter meaningful in diagnostics and avoid recording an
    // absolute, machine-specific source location in provenance.
    ensure!(
        data_dir.is_dir(),
        "data directory disappeared during import"
    );
    Ok(())
}

fn dataset_meta<T>(dataset: &Dataset<T>) -> Result<(&str, i64, i64)> {
    Ok((
        &dataset.source_path,
        to_i64(dataset.records.len(), "source record count")?,
        dataset.file_bytes,
    ))
}

fn create_schema(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TABLE schema_version(version INTEGER NOT NULL PRIMARY KEY);
        INSERT INTO schema_version(version) VALUES (4);
        CREATE TABLE migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL);
        INSERT INTO migrations(version, name, applied_at) VALUES (1, 'initial derived query schema', strftime('%Y-%m-%dT%H:%M:%fZ','now'));
        INSERT INTO migrations(version, name, applied_at) VALUES (2, 'item descriptions and artwork', strftime('%Y-%m-%dT%H:%M:%fZ','now'));
        INSERT INTO migrations(version, name, applied_at) VALUES (3, 'typed implant slots', strftime('%Y-%m-%dT%H:%M:%fZ','now'));
        INSERT INTO migrations(version, name, applied_at) VALUES (4, 'normalized mob schema', strftime('%Y-%m-%dT%H:%M:%fZ','now'));
        CREATE TABLE import_runs(
            id INTEGER PRIMARY KEY, schema_version INTEGER NOT NULL,
            imported_at TEXT NOT NULL
        );
        CREATE TABLE source_datasets(
            import_run_id INTEGER NOT NULL REFERENCES import_runs(id) ON DELETE CASCADE,
            dataset_name TEXT NOT NULL, source_path TEXT NOT NULL,
            record_count INTEGER NOT NULL CHECK(record_count >= 0),
            file_bytes INTEGER NOT NULL CHECK(file_bytes >= 0), imported_at TEXT NOT NULL,
            PRIMARY KEY(import_run_id, dataset_name)
        );

        CREATE TABLE items(
            slug TEXT PRIMARY KEY, name TEXT NOT NULL, item_type TEXT NOT NULL,
            description TEXT, image_url TEXT,
            implant_slot TEXT CHECK(implant_slot IN ('brain','heart','left-arm','right-arm','left-leg','right-leg')),
            damage_min INTEGER, damage_max INTEGER, attack_speed REAL,
            armor INTEGER, stamina INTEGER, strength INTEGER, agility INTEGER, intellect INTEGER,
            attack_power INTEGER, crit INTEGER, damage INTEGER, cast_time_reduction INTEGER,
            max_health INTEGER, max_energy INTEGER, health_regen INTEGER, energy_regen INTEGER,
            required_level INTEGER, required_stamina INTEGER, required_strength INTEGER,
            required_agility INTEGER, required_intellect INTEGER, required_faction_notoriety INTEGER,
            required_guild_level INTEGER, sellable_to_shops INTEGER NOT NULL CHECK(sellable_to_shops IN (0,1)),
            rebirth INTEGER NOT NULL CHECK(rebirth IN (0,1)), ascension INTEGER NOT NULL CHECK(ascension IN (0,1))
        );
        CREATE TABLE item_unknown_lines(
            item_slug TEXT NOT NULL REFERENCES items(slug) ON DELETE CASCADE,
            line_index INTEGER NOT NULL, raw_line TEXT NOT NULL,
            PRIMARY KEY(item_slug, line_index)
        );
        CREATE TABLE item_price_observations(
            item_slug TEXT NOT NULL REFERENCES items(slug) ON DELETE CASCADE,
            price_kind TEXT NOT NULL CHECK(price_kind IN ('market','recently_sold','shop')),
            price INTEGER NOT NULL CHECK(price >= 0), PRIMARY KEY(item_slug, price_kind)
        );

        CREATE TABLE skills(
            slug TEXT PRIMARY KEY, name TEXT NOT NULL, rank INTEGER, level_requirement INTEGER,
            cast_time_seconds REAL, duration_kind TEXT, duration_seconds REAL,
            cooldown_kind TEXT, cooldown_seconds REAL, energy_cost INTEGER
        );
        CREATE TABLE skill_quick_facts(
            skill_slug TEXT NOT NULL REFERENCES skills(slug) ON DELETE CASCADE,
            fact_index INTEGER NOT NULL, label TEXT NOT NULL, value TEXT NOT NULL,
            PRIMARY KEY(skill_slug, fact_index)
        );
        CREATE TABLE skill_requirements(
            skill_slug TEXT NOT NULL REFERENCES skills(slug) ON DELETE CASCADE,
            requirement_index INTEGER NOT NULL, attribute TEXT NOT NULL, amount INTEGER, raw_value TEXT NOT NULL,
            PRIMARY KEY(skill_slug, requirement_index)
        );
        CREATE TABLE skill_effects(
            id INTEGER PRIMARY KEY, skill_slug TEXT NOT NULL REFERENCES skills(slug) ON DELETE CASCADE,
            effect_index INTEGER NOT NULL, effect_type TEXT NOT NULL, details TEXT NOT NULL,
            UNIQUE(skill_slug, effect_index)
        );
        CREATE TABLE skill_effect_components(
            effect_id INTEGER NOT NULL REFERENCES skill_effects(id) ON DELETE CASCADE,
            component_index INTEGER NOT NULL, component_type TEXT NOT NULL,
            resource TEXT, direction TEXT, amount_min INTEGER, amount_max INTEGER,
            interval_seconds INTEGER, stat TEXT, signed_value INTEGER, multiplier INTEGER,
            unknown_raw TEXT, PRIMARY KEY(effect_id, component_index)
        );

        CREATE TABLE item_sets(
            slug TEXT PRIMARY KEY, name TEXT NOT NULL, bonus_rule TEXT NOT NULL, resource_tool_rule TEXT NOT NULL
        );
        CREATE TABLE item_set_pieces(
            set_slug TEXT NOT NULL REFERENCES item_sets(slug) ON DELETE CASCADE,
            piece_index INTEGER NOT NULL, piece_slug TEXT NOT NULL, item_slug TEXT REFERENCES items(slug),
            name TEXT NOT NULL, item_type TEXT NOT NULL, level_requirement INTEGER,
            PRIMARY KEY(set_slug, piece_index)
        );
        CREATE TABLE item_set_tiers(
            id INTEGER PRIMARY KEY, set_slug TEXT NOT NULL REFERENCES item_sets(slug) ON DELETE CASCADE,
            tier_index INTEGER NOT NULL, required_pieces INTEGER NOT NULL, UNIQUE(set_slug, tier_index)
        );
        CREATE TABLE item_set_effects(
            tier_id INTEGER NOT NULL REFERENCES item_set_tiers(id) ON DELETE CASCADE,
            effect_index INTEGER NOT NULL, stat TEXT NOT NULL, value TEXT NOT NULL,
            PRIMARY KEY(tier_id, effect_index)
        );

        CREATE TABLE mobs(
            slug TEXT PRIMARY KEY, name TEXT NOT NULL, source_url TEXT NOT NULL,
            level INTEGER, health INTEGER, damage_min INTEGER, damage_max INTEGER,
            attack_speed_ms INTEGER, attacks INTEGER CHECK(attacks IN (0,1)),
            faction_label TEXT, faction_url TEXT, faction_xp INTEGER,
            required_weapon_label TEXT, required_weapon_url TEXT,
            aggressive INTEGER CHECK(aggressive IN (0,1)), debuff_skill_count INTEGER,
            boss_candidate INTEGER NOT NULL CHECK(boss_candidate IN (0,1))
        );
        CREATE TABLE mob_debuffs(
            mob_slug TEXT NOT NULL REFERENCES mobs(slug) ON DELETE CASCADE,
            debuff_index INTEGER NOT NULL, debuff_label TEXT NOT NULL, source_url TEXT,
            skill_slug TEXT REFERENCES skills(slug), PRIMARY KEY(mob_slug, debuff_index)
        );
        CREATE TABLE zones(slug TEXT PRIMARY KEY, name TEXT NOT NULL, source_url TEXT);
        CREATE TABLE mob_locations(
            mob_slug TEXT NOT NULL REFERENCES mobs(slug) ON DELETE CASCADE,
            location_index INTEGER NOT NULL, zone_slug TEXT NOT NULL REFERENCES zones(slug),
            map_location_count INTEGER, zone_label TEXT NOT NULL,
            PRIMARY KEY(mob_slug, location_index)
        );
        CREATE TABLE mob_drop_profiles(
            id INTEGER PRIMARY KEY, mob_slug TEXT NOT NULL REFERENCES mobs(slug) ON DELETE CASCADE,
            profile_index INTEGER NOT NULL, summary_label TEXT NOT NULL, zone_label TEXT,
            zone_slug TEXT REFERENCES zones(slug), map_location_count INTEGER,
            solo_coins_min INTEGER, solo_coins_max INTEGER, notice TEXT,
            UNIQUE(mob_slug, profile_index)
        );
        CREATE TABLE mob_drops(
            id INTEGER PRIMARY KEY, profile_id INTEGER NOT NULL REFERENCES mob_drop_profiles(id) ON DELETE CASCADE,
            drop_index INTEGER NOT NULL, item_label TEXT NOT NULL, item_source_url TEXT,
            item_slug TEXT REFERENCES items(slug), solo_chance_at_least_one_percent REAL,
            maximum_quantity INTEGER, UNIQUE(profile_id, drop_index)
        );
        CREATE TABLE mob_drop_rolls(
            drop_id INTEGER NOT NULL REFERENCES mob_drops(id) ON DELETE CASCADE,
            roll_index INTEGER NOT NULL, source_label TEXT, chance_percent REAL,
            PRIMARY KEY(drop_id, roll_index)
        );

        CREATE INDEX item_prices_by_kind_price ON item_price_observations(price_kind, price);
        CREATE INDEX set_pieces_by_item ON item_set_pieces(item_slug);
        CREATE INDEX skill_effects_by_skill ON skill_effects(skill_slug);
        CREATE INDEX mobs_by_level ON mobs(level);
        CREATE INDEX mob_debuffs_by_skill ON mob_debuffs(skill_slug);
        CREATE INDEX mob_locations_by_zone ON mob_locations(zone_slug);
        CREATE INDEX drop_profiles_by_mob ON mob_drop_profiles(mob_slug);
        CREATE INDEX drops_by_item ON mob_drops(item_slug);
        CREATE INDEX rolls_by_source_chance ON mob_drop_rolls(source_label, chance_percent);

        CREATE VIEW item_prices AS
            SELECT i.slug AS item_slug, i.name, p.price_kind, p.price
            FROM items i JOIN item_price_observations p ON p.item_slug=i.slug;
        CREATE VIEW mob_item_drops AS
            SELECT m.slug AS mob_slug, m.name AS mob_name, p.profile_index, d.drop_index,
                   d.item_slug, d.item_label, d.solo_chance_at_least_one_percent, d.maximum_quantity
            FROM mobs m JOIN mob_drop_profiles p ON p.mob_slug=m.slug
            JOIN mob_drops d ON d.profile_id=p.id;
        CREATE VIEW mob_combat AS
            SELECT slug, name, level, health, damage_min, damage_max, attack_speed_ms, attacks,
                   aggressive, faction_label, faction_xp, required_weapon_label, debuff_skill_count,
                   boss_candidate
            FROM mobs;
        "#,
    )?;
    Ok(())
}

fn import_items(tx: &Transaction<'_>, items: &[Item]) -> Result<()> {
    for item in items {
        tx.execute(
            "INSERT INTO items VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32)",
            params![item.slug,item.name,item.item_type,item.description,item.image_url,item.implant_slot.map(|slot| slot.as_str()),item.damage_min,item.damage_max,finite_f32(item.attack_speed,"item attack speed")?,item.stats.armor,item.stats.stamina,item.stats.strength,item.stats.agility,item.stats.intellect,item.stats.attack_power,item.stats.crit,item.stats.damage,item.stats.cast_time_reduction,item.stats.max_health,item.stats.max_energy,item.stats.health_regen,item.stats.energy_regen,item.requirements.level,item.requirements.stamina,item.requirements.strength,item.requirements.agility,item.requirements.intellect,item.requirements.faction_notoriety,item.requirements.guild_level,item.sellable_to_shops,item.rebirth,item.ascension],
        ).with_context(|| format!("inserting item {}", item.slug))?;
        for (index, line) in item.unparsed_tooltip_lines.iter().enumerate() {
            tx.execute(
                "INSERT INTO item_unknown_lines VALUES (?1,?2,?3)",
                params![item.slug, to_i64(index, "unknown line index")?, line],
            )?;
        }
        for (kind, value) in [
            ("market", item.market_price),
            ("recently_sold", item.recently_sold_price),
            ("shop", item.shop_price),
        ] {
            if let Some(value) = value {
                tx.execute(
                    "INSERT INTO item_price_observations VALUES (?1,?2,?3)",
                    params![item.slug, kind, to_i64(value, "item price")?],
                )?;
            }
        }
    }
    Ok(())
}

fn import_skills(tx: &Transaction<'_>, skills: &[Skill]) -> Result<()> {
    for skill in skills {
        let (duration_kind, duration_seconds) = match skill.duration {
            None => (None, None),
            Some(SkillDuration::NoTimedDuration) => (Some("none"), None),
            Some(SkillDuration::Seconds(value)) => (
                Some("seconds"),
                finite(Some(f64::from(value)), "skill duration")?,
            ),
        };
        let (cooldown_kind, cooldown_seconds) = match skill.cooldown {
            None => (None, None),
            Some(SkillCooldown::None) => (Some("none"), None),
            Some(SkillCooldown::Seconds(value)) => (
                Some("seconds"),
                finite(Some(f64::from(value)), "skill cooldown")?,
            ),
        };
        tx.execute(
            "INSERT INTO skills VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                skill.slug,
                skill.name,
                skill.rank,
                skill.level_requirement,
                finite_f32(skill.cast_time, "skill cast time")?,
                duration_kind,
                duration_seconds,
                cooldown_kind,
                cooldown_seconds,
                skill.energy_cost
            ],
        )?;
        for (index, fact) in skill.quick_facts.iter().enumerate() {
            tx.execute(
                "INSERT INTO skill_quick_facts VALUES (?1,?2,?3,?4)",
                params![
                    skill.slug,
                    to_i64(index, "quick fact index")?,
                    fact.label,
                    fact.value
                ],
            )?;
        }
        for (index, req) in skill.attribute_requirements.iter().enumerate() {
            tx.execute(
                "INSERT INTO skill_requirements VALUES (?1,?2,?3,?4,?5)",
                params![
                    skill.slug,
                    to_i64(index, "skill requirement index")?,
                    req.attribute,
                    req.amount,
                    req.value
                ],
            )?;
        }
        for (effect_index, effect) in skill.effects.iter().enumerate() {
            tx.execute("INSERT INTO skill_effects(skill_slug,effect_index,effect_type,details) VALUES (?1,?2,?3,?4)", params![skill.slug,to_i64(effect_index,"skill effect index")?,effect.effect_type,effect.details])?;
            let effect_id = tx.last_insert_rowid();
            for (component_index, component) in effect.components.iter().enumerate() {
                insert_component(tx, effect_id, component_index, component)?;
            }
        }
    }
    Ok(())
}

fn insert_component(
    tx: &Transaction<'_>,
    effect_id: i64,
    index: usize,
    component: &SkillEffectComponent,
) -> Result<()> {
    let kind;
    let (
        mut resource,
        mut direction,
        mut min,
        mut max,
        mut interval,
        mut stat,
        mut signed,
        mut multiplier,
        mut unknown,
    ) = (None, None, None, None, None, None, None, None, None);
    match component {
        SkillEffectComponent::DirectDamage { amount } => {
            kind = "direct_damage";
            (min, max) = amount_values(amount);
        }
        SkillEffectComponent::DirectHeal { amount } => {
            kind = "direct_heal";
            (min, max) = amount_values(amount);
        }
        SkillEffectComponent::HealthLoss { amount } => {
            kind = "health_loss";
            (min, max) = amount_values(amount);
        }
        SkillEffectComponent::TimedStatModifier {
            stat: stat_name,
            value,
        } => {
            kind = "timed_stat_modifier";
            stat = Some(stat_name.as_str());
            signed = Some(i64::from(*value));
        }
        SkillEffectComponent::Periodic {
            resource: r,
            direction: d,
            amount,
            interval_seconds,
        } => {
            kind = "periodic";
            resource = Some(match r {
                EffectResource::Health => "health",
                EffectResource::Energy => "energy",
            });
            direction = Some(match d {
                EffectDirection::Gain => "gain",
                EffectDirection::Loss => "loss",
            });
            (min, max) = amount_values(amount);
            interval = Some(i64::from(*interval_seconds));
        }
        SkillEffectComponent::ThreatMultiplier { multiplier: value } => {
            kind = "threat_multiplier";
            multiplier = Some(i64::from(*value));
        }
        SkillEffectComponent::Teleport => kind = "teleport",
        SkillEffectComponent::Pet => kind = "pet",
        SkillEffectComponent::Morph => kind = "morph",
        SkillEffectComponent::Unknown { text } => {
            kind = "unknown";
            unknown = Some(text.as_str());
        }
    }
    tx.execute(
        "INSERT INTO skill_effect_components VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            effect_id,
            to_i64(index, "component index")?,
            kind,
            resource,
            direction,
            min,
            max,
            interval,
            stat,
            signed,
            multiplier,
            unknown
        ],
    )?;
    Ok(())
}

fn amount_values(amount: &EffectAmount) -> (Option<i64>, Option<i64>) {
    match amount {
        EffectAmount::Fixed(value) => (Some(i64::from(*value)), Some(i64::from(*value))),
        EffectAmount::Range { min, max } => (Some(i64::from(*min)), Some(i64::from(*max))),
    }
}

fn import_sets(tx: &Transaction<'_>, sets: &[ItemSet]) -> Result<()> {
    let items = existing_slugs(tx, "items")?;
    for set in sets {
        tx.execute(
            "INSERT INTO item_sets VALUES (?1,?2,?3,?4)",
            params![set.slug, set.name, set.bonus_rule, set.resource_tool_rule],
        )?;
        for (index, piece) in set.pieces.iter().enumerate() {
            let linked = items.contains(&piece.slug).then_some(piece.slug.as_str());
            tx.execute(
                "INSERT INTO item_set_pieces VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    set.slug,
                    to_i64(index, "set piece index")?,
                    piece.slug,
                    linked,
                    piece.name,
                    piece.item_type,
                    piece.level_requirement
                ],
            )?;
        }
        for (tier_index, tier) in set.bonuses.iter().enumerate() {
            tx.execute(
                "INSERT INTO item_set_tiers(set_slug,tier_index,required_pieces) VALUES (?1,?2,?3)",
                params![
                    set.slug,
                    to_i64(tier_index, "set tier index")?,
                    tier.required_pieces
                ],
            )?;
            let tier_id = tx.last_insert_rowid();
            for (effect_index, effect) in tier.effects.iter().enumerate() {
                tx.execute(
                    "INSERT INTO item_set_effects VALUES (?1,?2,?3,?4)",
                    params![
                        tier_id,
                        to_i64(effect_index, "set effect index")?,
                        effect.stat,
                        effect.value
                    ],
                )?;
            }
        }
    }
    Ok(())
}

fn import_mobs(tx: &Transaction<'_>, mobs: &[Mob]) -> Result<()> {
    let items = existing_slugs(tx, "items")?;
    let skills = existing_slugs(tx, "skills")?;
    for mob in mobs {
        let (damage_min, damage_max) = mob
            .damage
            .as_ref()
            .map(|r| -> Result<_> {
                Ok((
                    Some(to_i64(r.min, "mob damage")?),
                    Some(to_i64(r.max, "mob damage")?),
                ))
            })
            .transpose()?
            .unwrap_or((None, None));
        tx.execute(
            "INSERT INTO mobs(slug,name,source_url,level,health,damage_min,damage_max,attack_speed_ms,attacks,faction_label,faction_url,faction_xp,required_weapon_label,required_weapon_url,aggressive,debuff_skill_count,boss_candidate) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                mob.slug,
                mob.name,
                mob.source_url,
                opt_u64(mob.level, "mob level")?,
                opt_u64(mob.health, "mob health")?,
                damage_min,
                damage_max,
                opt_u64(mob.attack_speed_ms, "attack speed")?,
                mob.attacks,
                value_text(&mob.faction),
                value_href(&mob.faction),
                mob.faction_xp,
                value_text(&mob.required_weapon),
                value_href(&mob.required_weapon),
                mob.aggressive,
                opt_u64(mob.debuff_skill_count, "debuff count")?,
                mob.boss_candidate
            ],
        )?;
        for (index, debuff) in mob.debuffs.iter().enumerate() {
            let candidate = slug_from_value(debuff, "skills");
            let linked = candidate.as_deref().filter(|slug| skills.contains(*slug));
            tx.execute(
                "INSERT INTO mob_debuffs(mob_slug,debuff_index,debuff_label,source_url,skill_slug) VALUES (?1,?2,?3,?4,?5)",
                params![
                    mob.slug,
                    to_i64(index, "debuff index")?,
                    debuff.text,
                    first_href(debuff),
                    linked
                ],
            )?;
        }
        for (index, location) in mob.locations.iter().enumerate() {
            let zone_slug = slug_from_href(&location.zone.href, "zones")
                .unwrap_or_else(|| stable_fallback_slug(&location.zone.text));
            upsert_zone(
                tx,
                &zone_slug,
                &location.zone.text,
                Some(&location.zone.href),
            )?;
            tx.execute(
                "INSERT INTO mob_locations(mob_slug,location_index,zone_slug,map_location_count,zone_label) VALUES (?1,?2,?3,?4,?5)",
                params![
                    mob.slug,
                    to_i64(index, "location index")?,
                    zone_slug,
                    opt_u64(location.map_location_count, "map location count")?,
                    location.zone.text
                ],
            )?;
        }
        for (profile_index, profile) in mob.drop_profiles.iter().enumerate() {
            let zone_slug = profile
                .zone
                .as_ref()
                .and_then(|v| slug_from_value(v, "zones"));
            if let (Some(slug), Some(zone)) = (zone_slug.as_deref(), profile.zone.as_ref()) {
                upsert_zone(tx, slug, &zone.text, first_href(zone))?;
            }
            let (coin_min, coin_max) = profile
                .solo_coins
                .as_ref()
                .map(|r| -> Result<_> {
                    Ok((
                        Some(to_i64(r.min, "solo coins")?),
                        Some(to_i64(r.max, "solo coins")?),
                    ))
                })
                .transpose()?
                .unwrap_or((None, None));
            tx.execute("INSERT INTO mob_drop_profiles(mob_slug,profile_index,summary_label,zone_label,zone_slug,map_location_count,solo_coins_min,solo_coins_max,notice) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![mob.slug,to_i64(profile_index,"drop profile index")?,profile.summary.text,profile.zone.as_ref().map(|v|v.text.as_str()),zone_slug,opt_u64(profile.map_location_count,"profile map count")?,coin_min,coin_max,profile.notice])?;
            let profile_id = tx.last_insert_rowid();
            for (drop_index, drop) in profile.drops.iter().enumerate() {
                let candidate = slug_from_value(&drop.item, "items");
                let linked = candidate.as_deref().filter(|slug| items.contains(*slug));
                tx.execute("INSERT INTO mob_drops(profile_id,drop_index,item_label,item_source_url,item_slug,solo_chance_at_least_one_percent,maximum_quantity) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![profile_id,to_i64(drop_index,"drop index")?,drop.item.text,first_href(&drop.item),linked,finite(drop.solo_chance_at_least_one_percent,"drop chance")?,opt_u64(drop.maximum_quantity,"maximum quantity")?])?;
                let drop_id = tx.last_insert_rowid();
                for (roll_index, roll) in drop.rolls.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO mob_drop_rolls(drop_id,roll_index,source_label,chance_percent) VALUES (?1,?2,?3,?4)",
                        params![
                            drop_id,
                            to_i64(roll_index, "roll index")?,
                            roll.source,
                            finite(roll.chance_percent, "roll chance")?
                        ],
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn upsert_zone(tx: &Transaction<'_>, slug: &str, name: &str, url: Option<&str>) -> Result<()> {
    tx.execute("INSERT INTO zones(slug,name,source_url) VALUES (?1,?2,?3) ON CONFLICT(slug) DO UPDATE SET name=excluded.name, source_url=coalesce(zones.source_url,excluded.source_url)",params![slug,name,url])?;
    Ok(())
}

fn existing_slugs(tx: &Transaction<'_>, table: &str) -> Result<HashSet<String>> {
    ensure!(
        matches!(table, "items" | "skills"),
        "unsupported slug table"
    );
    let mut statement = tx.prepare(&format!("SELECT slug FROM {table}"))?;
    Ok(statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

fn validate_counts(
    tx: &Transaction<'_>,
    items: &Dataset<Item>,
    skills: &Dataset<Skill>,
    sets: &Dataset<ItemSet>,
    mobs: &Dataset<Mob>,
) -> Result<()> {
    for (table, expected) in [
        ("items", items.records.len()),
        ("skills", skills.records.len()),
        ("item_sets", sets.records.len()),
        ("mobs", mobs.records.len()),
    ] {
        let actual: i64 = tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })?;
        ensure!(
            actual == to_i64(expected, "expected record count")?,
            "{table}: expected {expected} rows, imported {actual}"
        );
    }
    Ok(())
}

fn value_text(value: &Option<MobValue>) -> Option<&str> {
    value.as_ref().map(|v| v.text.as_str())
}
fn value_href(value: &Option<MobValue>) -> Option<&str> {
    value.as_ref().and_then(first_href)
}
fn first_href(value: &MobValue) -> Option<&str> {
    value.links.first().map(|l| l.href.as_str())
}
fn slug_from_value(value: &MobValue, kind: &str) -> Option<String> {
    value
        .links
        .iter()
        .find_map(|l| slug_from_href(&l.href, kind))
}
fn slug_from_href(href: &str, kind: &str) -> Option<String> {
    let path = href.split(['?', '#']).next()?;
    let prefix = format!("/{kind}/");
    path.strip_prefix(&prefix)
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .map(str::to_string)
}
fn stable_fallback_slug(label: &str) -> String {
    let slug = label
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("unlinked-{slug}")
}
fn to_i64<T>(value: T, label: &str) -> Result<i64>
where
    i64: TryFrom<T>,
{
    i64::try_from(value).map_err(|_| anyhow::anyhow!("{label} exceeds SQLite INTEGER range"))
}
fn opt_u64(value: Option<u64>, label: &str) -> Result<Option<i64>> {
    value.map(|v| to_i64(v, label)).transpose()
}
fn finite(value: Option<f64>, label: &str) -> Result<Option<f64>> {
    match value {
        Some(v) if !v.is_finite() => bail!("{label} is not finite"),
        other => Ok(other),
    }
}
fn finite_f32(value: Option<f32>, label: &str) -> Result<Option<f64>> {
    finite(value.map(f64::from), label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("fo2-storage-test-{}-{n}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_sources(dir: &Path, mobs: &str) {
        fs::write(dir.join("items.json"),r#"[{"name":"Coin","slug":"coin-1","item_type":"Misc","damage_min":null,"damage_max":null,"attack_speed":null,"stats":{"armor":null,"stamina":null,"strength":null,"agility":null,"intellect":null,"attack_power":null,"crit":null,"damage":null,"cast_time_reduction":null,"max_health":null,"max_energy":null,"health_regen":null,"energy_regen":null},"requirements":{"level":null,"stamina":null,"strength":null,"agility":null,"intellect":null,"faction_notoriety":null,"guild_level":null},"market_price":12,"recently_sold_price":null,"shop_price":null,"sellable_to_shops":true,"rebirth":false,"ascension":false,"unparsed_tooltip_lines":[]}]"#).unwrap();
        fs::write(dir.join("skills.json"),r#"[{"name":"Slow","slug":"slow-1","rank":null,"quick_facts":[],"level_requirement":null,"attribute_requirements":[],"cast_time":null,"duration":null,"cooldown":null,"energy_cost":null,"effects":[]}]"#).unwrap();
        fs::write(dir.join("item-sets.json"), "[]").unwrap();
        fs::write(dir.join("mobs.json"), mobs).unwrap();
    }

    #[test]
    fn keeps_repeated_rolls_and_nullable_unlisted_debuff_fk() {
        let dir = TempDir::new();
        let mobs = r#"[{"slug":"rat-1","name":"Rat","source_url":"https://example/mobs/rat-1","level":1,"health":2,"damage":null,"attack_speed_ms":null,"attacks":null,"faction":null,"faction_xp":null,"required_weapon":null,"aggressive":null,"debuff_skill_count":2,"debuffs":[{"text":"Slow","links":[{"href":"/skills/slow-1","text":"Slow","aria_label":null}]},{"text":"Unlisted","links":[]}],"locations":[{"zone":{"href":"/zones/sewers","text":"Sewers","aria_label":null},"map_location_count":1}],"drop_profiles":[{"summary":{"text":"one","links":[]},"zone":null,"map_location_count":null,"solo_coins":null,"drops":[{"item":{"text":"Coin","links":[{"href":"/items/coin-1","text":"Coin","aria_label":null}]},"rolls":[{"source":"Spawn","chance_percent":50.0},{"source":"Spawn","chance_percent":50.0}],"solo_chance_at_least_one_percent":75.0,"maximum_quantity":2}],"notice":"Solo drop notice"}],"boss_candidate":true}]"#;
        write_sources(&dir.0, mobs);
        build_database_in(&dir.0).unwrap();
        let db = Connection::open(dir.0.join("fo2.sqlite")).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM mob_drop_rolls", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            db.query_row("SELECT version FROM schema_version", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            db.query_row("SELECT name FROM migrations WHERE version = 4", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "normalized mob schema"
        );
        assert_eq!(
            db.query_row("SELECT boss_candidate FROM mob_combat", [], |r| {
                r.get::<_, bool>(0)
            })
            .unwrap(),
            true
        );
        assert_eq!(
            db.query_row("SELECT zone_label FROM mob_locations", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "Sewers"
        );
        assert_eq!(
            db.query_row("SELECT notice FROM mob_drop_profiles", [], |r| {
                r.get::<_, Option<String>>(0)
            })
            .unwrap(),
            Some("Solo drop notice".to_owned())
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM mob_debuffs WHERE skill_slug IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }

    #[test]
    fn failed_rebuild_preserves_existing_database() {
        let dir = TempDir::new();
        write_sources(&dir.0, "[]");
        build_database_in(&dir.0).unwrap();
        let before = fs::read(dir.0.join("fo2.sqlite")).unwrap();
        fs::write(dir.0.join("mobs.json"), "not json").unwrap();
        assert!(build_database_in(&dir.0).is_err());
        assert_eq!(fs::read(dir.0.join("fo2.sqlite")).unwrap(), before);
        assert!(
            !fs::read_dir(&dir.0).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }
}
