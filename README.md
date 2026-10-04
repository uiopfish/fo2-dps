# Fantasy Online 2 DPS & Grinding Calculator

A data-backed character-build, combat, and farming calculator for [Fantasy Online 2](https://fantasyonline2.com/). The application combines a Rust calculation engine, a responsive browser interface, WebAssembly support for static hosting, and a resumable collection pipeline for data from the [Fantasy Online 2 Player Database](https://db.fantasyonline2.com/).

**Web app:** https://uiopfish.github.io/fo2-dps/

The calculator can inspect and share character builds, derive confirmed combat stats, compare grinding routes, and explore the underlying item, skill, set, mob, drop, and economy data. Unknown mechanics remain explicit instead of being replaced with guessed formulas.

## Table of contents

- [Features](#features)
- [Web application](#web-application)
- [Commands](#commands)
- [Static GitHub Pages deployment](#static-github-pages-deployment)
- [Character build inspection](#character-build-inspection)
- [Combat-cycle assumptions](#combat-cycle-assumptions)
- [Grinding estimates](#grinding-estimates)
- [Mob snapshots](#mob-snapshots)
- [Bulk collection](#bulk-collection)
- [Coverage and limitations](#coverage-and-limitations)
- [Dataset validation](#dataset-validation)
- [SQLite query database](#sqlite-query-database)
- [Validation](#validation)

## Features

- Browser-based Build Lab with equipment, implant, buff, morph, pet, and skill selection
- Live build validation and inspectable derived-stat breakdowns
- Expected-event combat-cycle estimates used by Grinding
- Grinding leaderboards and route comparisons for gold, pure coin drops, and faction XP per hour
- Searchable item, mob, skill, and item-set explorer
- Portable build import/export, self-contained build codes/share URLs, and browser-local workspace persistence
- Serverless WebAssembly deployment with no hosted API or database
- Native Rust CLI, local HTTP application, data validation, scraping, and SQLite export tools

## Web application

Use the hosted calculator at https://uiopfish.github.io/fo2-dps/ or start the native local application with:

```sh
cargo run -- serve
```

Then open http://127.0.0.1:8787. A custom bind address can be supplied with `cargo run -- serve 127.0.0.1:9000`. Keep the default loopback binding unless you intentionally want other machines to reach the unauthenticated local API.

The same dependency-free frontend runs through WebAssembly on GitHub Pages or is embedded into the native Rust server. It provides:

- Archive dashboard with live dataset counts
- Searchable, paginated item, mob, skill, and item-set explorer
- Mob details backed by the tracked normalized dataset; raw source evidence remains outside the web application
- Shared, browser-persisted character build workspace
- Build inspection and validation

- Single-target grinding estimates
- Multi-route grinding comparisons
- Responsive layouts, persistent desktop-only Yokou Mode, keyboard-accessible navigation, loading/error states, and reduced-motion support

Guided forms are the default workflow: progression and attribute inputs, visual equipment and active-effect slot boards, required-level-sorted item/skill pickers, Grinding-owned combat assumptions, route policies, and consumable-cost rows. Fresh builds start at level 1 with 20 base points in every attribute and zero allocated points. Available, assigned, and unassigned allocation points update immediately from the Spawn/Rebirth/Ascension budget formulas. Guided allocation inputs cannot fall below zero or exceed the remaining budget; lowering level or progression trims excess allocations, while negative equipment modifiers may still reduce an inspected final attribute below its base value of 20. Displayed attribute totals start at base 20 plus allocations, then refresh with item/set totals from automatic server inspection. Clicking an equipment, outfit, or implant slot opens a searchable picker restricted to compatible item categories, with requirements and detail links. The six-slot Implants tab appears only for Ascension builds and uses the game's designated Brain, Heart, Left Arm, Right Arm, Left Leg, and Right Leg positions. Implant pickers and build validation enforce the body-part category from each item's typed `implant_slot`, derived from its published description; an implant cannot be equipped into another body-part slot. Build validation is live and debounced: level, progression, allocations, equipment, skills, active roles, imported JSON, and slot changes automatically refresh validation and stat breakdowns without an Inspect button. `sacred-gauntlet-implant-1492` is classified as Left Arm from its published “Implant for your Left Arm.” description, not from its slug. All 68 current implants have exactly one description-backed designated slot. Outfit choices are cosmetic/local and do not enter combat calculations. Advanced JSON panels remain available for exact schema editing and troubleshooting. Character builds can be downloaded or imported as portable JSON files, encoded into versioned self-contained build codes, or shared through `?build=...#build` URLs; cosmetic outfits remain local. All workspaces and the desktop-only Yokou Mode density preference persist locally in the browser.

The build result is a dedicated character sheet rather than a generic metric sample. It shows confirmed maximum Health and Energy from total post-equipment attributes, raw Armor, Attack Power when final-attribute leadership is unambiguous, basic-attack panel damage, attack interval, bounded two-second out-of-combat Health/Energy Regen candidates, a base/allocated/gear/set/total attribute table, direct modifiers, active set tiers, selected skills, validation errors, and unresolved mechanics. Basic-attack damage per hit uses total Attack Power, the weapon's unbuffed base attack interval, weapon damage, and equipped implant Damage; attack-speed buffs affect attack frequency and DPS without reducing damage per hit. Skill/buff/pet periodic regeneration remains separately effect-defined and always active for the effect duration. The all-source Crit soft cap and piecewise Dodge rule are active; the conflicting level-103 Crit screenshot reading and equipment-regeneration interaction remain documented rather than hidden.

Read-only/list APIs include `/api/summary`, `/api/items`, `/api/mobs`, `/api/skills`, and `/api/item-sets`, with `search`, `offset`, and `limit` query parameters. Calculator APIs are `/api/build/inspect`, `/api/grind/{mob-slug}`, `/api/grind/compare`, and `/api/grind/leaderboard`. Requests are limited to 2 MiB. Native startup reads the tracked normalized snapshots, including `data/mobs.json`; it does not load the separate raw mob archive.

## Commands

```sh
cargo run -- item crystal-dragon-legs-1355
cargo run -- skill vampiric-blade-77
cargo run -- item-set crystal-dragon-14
cargo run -- item-sets
cargo run -- items-audit
cargo run -- items
cargo run -- skills
cargo run -- mobs
cargo run -- normalize-data
cargo run -- validate-data
cargo run -- build-db
cargo run -- build-web-bundle
cargo run -- assemble-pages
cargo run -- inspect-build path/to/build.json

cargo run -- grind path/to/build.json <mob-slug> path/to/grinding-assumptions.json
cargo run -- grind-compare path/to/build.json path/to/comparison.json
cargo run -- serve
```

`item-sets` writes `data/item-sets.json`. `items-audit` reads that dataset and fetches its distinct item slugs into `data/item-audit.json`, without replacing the full item dataset.

## Static GitHub Pages deployment

The same API contracts and Rust calculations can run entirely in the browser through WebAssembly. `build-web-bundle` combines the already-normalized authoritative snapshots into `web/data/app-data.v1.json` without destructively compacting mob records. The generated file currently uses bundle schema version 2 and is about 4.17 MiB for all 3,203 items, 21 sets, 367 skills, and 467 mobs. `scripts/build-pages.sh` assembles `dist/`, compiles the browser-safe Rust library, and generates the JavaScript WebAssembly bindings. Pages never includes or downloads `data/mobs.raw.jsonl.gz`; the resulting site makes no `/api` requests and needs no server, billing account, database, or paid service.

After a successful scrape, refresh and validate the deployment data with:

```sh
cargo run -- normalize-data
cargo run -- validate-data
cargo run -- build-web-bundle
./scripts/build-pages.sh
```

The workflow in `.github/workflows/pages.yml` deploys `dist/` through GitHub Pages. See `docs/deployment.md` for one-time repository setup, local testing, update procedures, and the no-billing guarantees and limitations.

## Character build inspection

`inspect-build` validates a JSON character build against the item, set, and skill snapshots and prints an inspectable breakdown. Builds contain level, progression, allocated attributes, typed equipment slots, selected skill slugs, and optional active skill roles:

```json
{
  "level": 60,
  "progression": "spawn",
  "allocated": {
    "stamina": 0,
    "strength": 0,
    "agility": 100,
    "intellect": 0
  },
  "equipment": [],
  "skills": ["alienated-92"],
  "active_skill_effects": [
    { "skill_slug": "alienated-92", "role": "morph" }
  ],
  "faction_notoriety": null,
  "guild_level": null
}
```

Valid active roles are `buff`, `pet`, and `morph`. The guided Build Lab uses five equipment-style buff slots and one pet slot; picker results show each skill's rank. At most five buffs and one pet may be active, and pets require Rebirth or Ascension progression. A role must reference a selected skill with a matching typed effect component. Buffs and morphs may be externally supplied, so their attribute requirements are not imposed on the receiving build; pets and attack skills still require the build's current attributes. Typed attribute, Armor, Attack Power, Crit, Max Health, Max Energy, and strongest attack-speed modifiers from active effects are included in derived stats. Active Crit shares the same all-source 80% soft cap as gear and set Crit. Periodic regeneration remains effect-defined, while movement speed and damage absorption are not folded into combat estimates.

The output separates `item_stats`, `set_bonus_stats`, `requirement_attributes`, and post-set `final_attributes`. Item attribute requirements are informational because Fantasy Online 2 permits overequipping with temporary stats; level, progression, faction, and guild restrictions remain validated. Locally cast pet and attack-skill attribute requirements use `requirement_attributes`; whether active set bonuses may satisfy them is not confirmed. Indexed slots preserve repeated equipment without inventing maximum ring/implant counts. Exact repeated-slot limits, two-handed/off-hand exclusion, and skill-slot limits are therefore not yet enforced.

## Combat-cycle assumptions

Grinding uses the tested encounter engine internally to resolve combat-cycle duration, survival, and expected damage. The standalone Encounter tab/API/CLI is intentionally not exposed; mechanics that are not confirmed remain explicit Grinding inputs rather than being inferred:

```json
{
  "assume_player_survives": false,
  "simultaneous_event_order": "player_first",
  "expected_noncritical_player_hit": 8.0,
  "player_crit_percent": 6.43,
  "player_attack_interval_seconds": 1.6,
  "player_max_health": 684.0,
  "player_dodge_percent": 5.0,
  "expected_post_mitigation_mob_hit": null,
  "energy": {
    "initial_energy": 760.0,
    "net_cost_per_second": 0.0
  }
}
```

Basic-attack integers are uniformly distributed across the inclusive panel range and mobs do not reduce outgoing player damage, so Build Lab supplies `expected_noncritical_player_hit` as the range midpoint. For incoming attacks, the model uses the midpoint of the published mob-damage range and applies `damage × (200 + level × 50) / ((200 + level × 50) + Armor)`. Dodge then determines the landed probability. `expected_post_mitigation_mob_hit` remains an optional advanced override. Setting `assume_player_survives` to `true` explicitly skips all incoming damage; it is a fallback rather than the default. Energy is optional and uses a scenario-supplied net cost per second.

The web Grinding tab automatically reuses safely derived values from the current Build Lab inspection, including midpoint basic damage, maximum Health, attack interval, Crit, Dodge, level, and Armor. The server and CLI overwrite any client-supplied defense context with the inspected build values. Armor mitigation is bypassed only through the visible survival assumption; active-effect stacking remains an explicit input boundary. Route comparisons apply the same Build Lab-derived values to each target at submission. The model applies the confirmed chained Crit expectation, attack intervals, incoming Dodge probability, and immediate opening player hit. `simultaneous_event_order` must be `player_first` or `mob_first` and explicitly controls equal attack timestamps. It reports expected damage per attack, analytical outgoing/incoming DPS and time to kill, plus an expected-damage event timeline with attack counts, elapsed time, defeat outcome, remaining health, and optional energy sustainability. The timeline resolves expected damage rather than random individual rolls; mob debuffs, skill rotations, healing, regeneration ticks, absorption, movement, latency, and recovery are not modeled.

## Grinding estimates

The web Grinding tab opens with an automatic top-50 leaderboard. It evaluates every published drop profile for every eligible mob, keeps each mob's best route, and can rank by known gold per hour, pure published coin drops per hour, or faction XP per hour. Mining, unlocking, and wood-cutting targets are excluded using typed required-weapon metadata. The explicit normalized `boss_candidate` boolean is derived solely from the archived source's `achievement-boss-` marker and is used to exclude likely bosses; this classification remains heuristic. The MVP leaderboard explicitly assumes the player survives, uses the current build's basic-attack output, and defaults to zero travel/recovery/respawn time. Leaderboard gold/hour is a known-value subtotal: published coin income plus drops with a known probability and selected price; unknown drop values contribute zero. Detailed single-route estimates retain strict null propagation. Clicking the Gold/h, Pure coins/h, or Faction XP/h table heading reruns the leaderboard using that ordering.

`grind` extends an encounter scenario with one location-specific drop profile and explicit cycle/economy policies:

```json
{
  "encounter": {
    "assume_player_survives": false,
    "simultaneous_event_order": "player_first",
    "expected_noncritical_player_hit": 8.0,
    "player_crit_percent": 6.43,
    "player_attack_interval_seconds": 1.6,
    "player_max_health": 684.0,
    "player_dodge_percent": 5.0,
    "expected_post_mitigation_mob_hit": null,
    "energy": null
  },
  "drop_profile_index": 0,
  "travel_seconds_per_kill": 1.0,
  "recovery_seconds_per_kill": 2.0,
  "respawn_wait_seconds_per_kill": 5.0,
  "coin_expectation_model": "midpoint_of_published_range",
  "drop_quantity_model": "one_per_successful_roll",
  "price_model": "shop",
  "recently_sold_threshold_multiplier": 3.0,
  "consumables": [
    {
      "label": "Potion",
      "expected_quantity_per_kill": 0.25,
      "unit_cost": 100.0
    }
  ]
}
```

Each successful independent drop roll always contributes one item and one loot click; legacy quantity-model values remain accepted in saved JSON but are normalized to `one_per_successful_roll`. The guided UI exposes `shop` and `shop_or_recently_sold`. Hybrid valuation uses the recently-sold observation only when it is at least `recently_sold_threshold_multiplier × shop_price` (default `3`); otherwise it uses shop value. If no shop price exists, hybrid mode uses a recently-sold observation when available. Legacy price models remain deserializable for compatibility. Missing roll probabilities, item links, or selected prices propagate to `null` detailed totals rather than zero. Repeated drop rolls remain separate. Expected loot clicks/hour is the sum of successful-roll expectations per kill times kills/hour; an active maximum-click filter excludes routes with unknown clicks as well as routes above the limit.

Kills/hour uses encounter time plus the explicit travel, recovery, and respawn-wait inputs. An opening one-shot has zero combat-cycle time because attack timing resets on kill. When the full cycle is zero, the selected profile's published map-spawn count automatically supplies the finite `spawn count × 120 kills/hour` rate; positive-duration routes retain the optional cap. Market and recently-sold values remain observations rather than guaranteed proceeds. Consumables use explicit expected quantities and unit costs and are subtracted from gross coins-plus-items to produce net value per kill/hour.

Boss detection is currently a bounded heuristic, not typed boss metadata. Actual boss/miniboss spawn timers, including the reported 10+ minute Gamer's Right Hand respawn, still need authoritative data before those targets can be ranked reliably.

`grind-compare` accepts one build and multiple complete grinding targets:

```json
{
  "ranking_metric": "expected_net_value_per_hour",
  "targets": [
    {
      "mob_slug": "angry-skele-8",
      "assumptions": {
        "encounter": {
          "assume_player_survives": false,
          "simultaneous_event_order": "player_first",
          "expected_noncritical_player_hit": 8.0,
          "player_crit_percent": 6.43,
          "player_attack_interval_seconds": 1.6,
          "player_max_health": 684.0,
          "player_dodge_percent": 5.0,
          "expected_post_mitigation_mob_hit": null,
          "energy": null
        },
        "drop_profile_index": 0,
        "travel_seconds_per_kill": 1.0,
        "recovery_seconds_per_kill": 2.0,
        "respawn_wait_seconds_per_kill": 5.0,
        "coin_expectation_model": "midpoint_of_published_range",
        "drop_quantity_model": "one_per_successful_roll",
        "price_model": "shop",
        "consumables": []
      }
    }
  ]
}
```

Ranking metrics are `kills_per_hour`, `expected_coins_per_hour`, `expected_item_value_per_hour`, `expected_gross_value_per_hour`, and `expected_net_value_per_hour`. Each target has independent combat, location, downtime, valuation, and consumable assumptions; drop quantity is fixed at one item per successful roll. Targets with unknown selected metrics or invalid scenarios remain unranked with their null/error details preserved; they are never assigned a zero value.

## Mob snapshots

`cargo run -- mob <slug>` fetches and prints one normalized mob. `cargo run -- mobs` discovers all mob index pages and, after a complete pass, publishes four outputs using the same resumable runner as items and skills: normalized `data/mobs.json`, raw `data/mobs.raw.jsonl.gz`, tracked `data/mobs.provenance.json`, and `data/mobs.scrape-report.json`.

The standalone model in `src/mobs.rs` stores combat stats, attack intervals in milliseconds, explicit non-attacking status, faction XP, aggression, weapon restrictions, debuffs, zone links, location-specific drop profiles, and an explicit `boss_candidate` boolean. Drop profiles retain solo coin ranges, displayed drop percentages, maximum quantities, published map-spawn counts, and ordered independent rolls. Repeated rolls must not be deduplicated. `boss_candidate` is derived solely from the raw archive's `achievement-boss-` marker; it is a bounded heuristic, not an authoritative boss classification.

The tracked `data/mobs.json` is the normalized 467-record application dataset, about 2.5 MiB. It contains no source HTML, raw tables, or raw cells. Lossless source evidence is written separately as gzip-compressed JSON Lines in `data/mobs.raw.jsonl.gz`, currently about 5.8 MiB. That archive is gitignored: collection creates it locally, but the repository and GitHub Pages do not publish it, and this project does not claim it is uploaded as a release asset. Retain or back it up separately when future reparsing or source inspection matters.

The tracked `data/mobs.provenance.json` records the record count, paths, archive format, and SHA-256 checksums of both the normalized snapshot and the exact raw archive produced by the same collection. It lets a locally retained archive be verified, but it is not a substitute for the archive itself. The derived SQLite database provides indexed relational tables for mobs, locations, drop profiles, rolls, items, skills, and price observations. Native startup, SQLite import, and the web-bundle build all read normalized `data/mobs.json`; the raw archive is evidence for later parsing rather than an application input.

## Bulk collection

`items`, `skills`, `mobs`, and `items-audit` checkpoint successful records to their corresponding `*.partial.jsonl` files. Rerun the same command to resume an incomplete run; already checkpointed slugs are skipped. Do not run concurrent copies of the same command.

Final application datasets are replaced only when all discovered records succeed. A completed checkpoint is removed; running again after completion starts a new collection. Failed runs return a nonzero exit status and leave the old final dataset intact. Reports are written to `*.scrape-report.json` at the end of a pass. Interrupted passes retain successful checkpoint records but may not have an updated report. On a successful `mobs` pass, the collector publishes the normalized snapshot, gzip JSONL raw archive, provenance manifest, and report together as the collection outputs.

Item and skill discovery follows pagination links rather than fixed page counts. Requests are sequential, with delays between detail/index requests, bounded HTTP timeouts, and exponential retries for transient failures.

Checkpoint schemas must match the current code. Archive incompatible checkpoints before restarting after a schema change; do not silently fill absent data with invented defaults. `data/skills.legacy.partial.jsonl` preserves the initial partial scrape, and `data/items.legacy.json` preserves the earlier item snapshot.

## Coverage and limitations

Successful fetching does not imply every game mechanic is typed or verified. Items retain unfamiliar tooltip lines in `unparsed_tooltip_lines`, with occurrence counts in the scrape report. Skill effects retain their effect type and raw details; requirements retain their raw values alongside optional numeric amounts. Skill quick facts are also preserved. Durations and cooldown values are normalized to seconds, including milliseconds and instant casts.

`normalize-data` upgrades the saved item, optional item-audit, and skill snapshots offline using the current parsers. Each output is atomically replaced (the group is not a multi-file transaction). Run it without concurrent collection commands. Recognized item lines move into typed fields; skill descriptions remain intact alongside derived `components`. Repeated normalization does not duplicate components. It writes current coverage to `data/normalization-report.json`; existing scrape reports remain historical reports of the original fetch.

The current 3,203-item snapshot includes the published item description and artwork URL for every item. Typed item data includes designated implant slots, flat damage bonuses (separate from weapon damage ranges), cast-time reduction, health/energy regeneration and maxima, Ascension, and guild requirements. Skill components cover direct amounts/ranges, periodic resource changes, timed stat modifiers, threat multipliers, teleport, and pet/morph markers. Range endpoints retain the source values without assuming an inclusive maximum. Modifier values retain DB units; normalization does not establish scaling, stacking, or target semantics.

The current normalized snapshots retain fishing-related item lines and one energy-transfer skill description as untyped data. Unknown effect fragments become explicit `Unknown` components rather than being silently discarded. Review `data/normalization-report.json` before using these records in damage calculations.

The item parser uses the tooltip container and item-type line, the semantic description class prefix, and Open Graph artwork metadata. A clean unknown-line report is not proof that the site exposes all game mechanics. Skill/debuff effect semantics and game-wide damage rules still need calculator implementation.

## Dataset validation

`cargo run -- validate-data` reads the current item, skill, set, and mob snapshots without modifying them, then atomically writes `data/validation-report.json`. Confirmed structural or relationship errors produce a nonzero exit status after the report is written; coverage warnings do not fail the command.

Checks include duplicate/malformed identities, drop-to-item, set-piece-to-item and debuff-to-skill links, zone consistency, numeric bounds and timing, explicit missing/null/zero/no-timing states, retained unknown fields, and a conservative legacy-item comparison. Repeated independent drop rolls are valid and are not summed or deduplicated.

The current snapshot has zero confirmed validation errors. Warnings remain for explicitly unlisted mob debuffs, set thresholds above the number of distinct listed pieces, ambiguous duplicate names in the slugless legacy snapshot, fishing tooltip data, and one untyped energy-transfer effect.

The project roadmap is recorded in `docs/roadmap.md`. Confirmed rules, unresolved formulas, sources, and the measurement plan are maintained in `docs/mechanics.md`; calculator code must not silently replace those unknowns with guesses.

## SQLite query database

`cargo run -- build-db` rebuilds `data/fo2.sqlite` from the four authoritative normalized JSON snapshots. SQLite is derived output and is gitignored; deleting it does not affect those tracked snapshots. The separate gitignored mob raw archive is not required for rebuilding SQLite and must be retained independently if its source evidence is needed.

The importer uses schema version 4, one transaction, source-dataset provenance, foreign keys, checked numeric conversion, count verification, `foreign_key_check`, and `integrity_check`. It builds and verifies a unique sibling temporary database before atomically replacing the previous database, so failed rebuilds preserve the last good copy.

The normalized schema covers item descriptions, artwork URLs, typed implant slots, separate price observations, item sets, skill facts/requirements/effects/components, mob combat and debuffs, zones and locations, location-specific drop profiles, drops, ordered independent rolls, and the explicit mob boss-candidate field. It imports normalized `data/mobs.json` and excludes the separate raw archive. Useful views include `item_prices`, `mob_item_drops`, and `mob_combat`.

Price kinds are `market`, `recently_sold`, and `shop`; none imply a guaranteed future sale. Published map-spawn counts remain separate from the derived 30-second throughput cap, and repeated drop rolls remain separate records.

Example:

```sh
sqlite3 -readonly data/fo2.sqlite \
  "SELECT mob_name, item_label, solo_chance_at_least_one_percent FROM mob_item_drops LIMIT 10;"
```

## Validation

```sh
cargo fmt --check
cargo check --offline
cargo test --offline
```

Offline tests cover parser formats, pagination, retry classification, checkpoint recovery/resume, and atomic publication. They do not replace live coverage review.
