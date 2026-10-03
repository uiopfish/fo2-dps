# FO2 Calculator Roadmap

This roadmap turns the scraped Fantasy Online 2 snapshots into a validated, queryable combat and grinding calculator. Each phase should preserve raw source evidence in the appropriate separate archive, identify assumptions explicitly, and avoid presenting unknown mechanics as facts.

## Table of contents

- [1. Validate and connect the datasets](#1-validate-and-connect-the-datasets--initial-milestone-complete)
- [2. Add SQLite as the application query layer](#2-add-sqlite-as-the-application-query-layer--initial-schema-complete)
- [3. Establish and document game rules](#3-establish-and-document-game-rules--in-progress)
- [4. Build the character/loadout model](#4-build-the-characterloadout-model--initial-milestone-complete)
- [5. First calculation milestone: one character versus one mob](#5-first-calculation-milestone-one-character-versus-one-mob--initial-milestone-complete)
- [6. Grinding and economy comparisons](#6-grinding-and-economy-comparisons--initial-milestone-complete)
- [7. Web application and guided UX](#7-web-application-and-guided-ux--in-progress)

## 1. Validate and connect the datasets — initial milestone complete

Validate relationships and data quality before adding formulas:

- Mob drops → items
- Item-set pieces → items
- Mob debuffs → skills
- Mob locations and drop-profile zones → consistent zone identifiers
- Numeric ranges, probabilities, and timing values → valid bounds
- Missing values → distinguish unknown, not applicable, explicit false/no-timing states, and numeric zero
- New and legacy item snapshots → report differences without assuming which source is complete

Deliverable: a repeatable `validate-data` command and `data/validation-report.json`. Validation reports issues but never removes or rewrites source records.

Current status: validation covers 3,162 items, 335 canonical scraped skills, 23 source-backed supplemental pet ranks (358 merged skills), 21 sets, and 467 mobs. All resolvable item-drop, set-piece, skill-debuff, and zone relationships pass with zero confirmed errors. Supplemental records use teaching-item slugs rather than invented canonical IDs and are merged with duplicate-slug rejection. Remaining warnings document source limitations and intentionally untyped data; they are not silently treated as valid mechanics. A fresh public-database rescan and teaching-item audit remain necessary because several taught utility/teleport ranks are absent from the canonical snapshot.

## 2. Add SQLite as the application query layer — initial schema complete

Keep normalized JSON snapshots as application/interchange data, preserve lossless mob source evidence in its separate gzip JSONL archive, and generate a versioned SQLite database for application queries.

Candidate areas:

- Provenance: imports and source records
- Equipment: items, stats, requirements
- Sets: sets, pieces, tiers, effects
- Skills: requirements, quick facts, raw and typed effects
- Mobs: combat data, debuffs, locations
- Rewards: location-specific profiles, drops, independent rolls
- Economy: price observations separated from permanent item data

Use stable slugs/IDs, preserve repeated drop rolls, retain unknown/raw values, import transactionally, and add indexes and schema migrations.

Deliverable: a rebuildable database and `build-db` command. SQLite is derived data; JSON snapshots remain the source of truth.

Current status: `build-db` transactionally imports the current normalized snapshots into `data/fo2.sqlite`, records source provenance, validates source/table counts, and runs SQLite integrity and foreign-key checks before atomic publication. The initial schema includes indexed item, price, set, skill/effect, mob, zone, debuff, location, drop-profile, drop, and independent-roll tables plus query views. Schema version 4 includes item descriptions, artwork URLs, description-backed typed implant slots, the normalized mob schema, and its explicit boss-candidate field. Future schema changes should continue to be recorded as migrations rather than destructive assumptions.

## 3. Establish and document game rules — in progress

Create a versioned mechanics specification with sources and confidence levels for:

- Attribute contributions
- Attack power and flat damage
- Critical chance and critical damage
- Armor, mitigation, hit/miss, and damage floors
- Attack-speed and cast-time modifiers
- Skill scaling, targeting, and rounding
- Buff/set/pet/morph stacking
- Resource consumption and regeneration
- Cast-request limits

Deliverable: rules documentation and executable test cases. Unknown mechanics stay explicitly unknown.

Current status: `src/mechanics.rs` implements published attribute-allocation budgets, the confirmed basic-attack panel formula, chained Crit and the all-source soft cap, Armor mitigation, attack intervals with strongest-only speed effects, highest-eligible set tiers with gathering-tool suppression, and bounded Health/Energy regeneration candidates. Active skills use published base damage and Crit as their only player-stat scaling; Attack Power does not affect them. Build inspection recommends the top three distinct locally castable attack skills using published timing, one-cast Energy eligibility, active in-combat Energy regeneration, and separate burst/sustained DPS scores. Remaining bounded questions are listed in `docs/mechanics.md` rather than reopening the confirmed weapon/basic-attack model.

## 4. Build the character/loadout model — initial milestone complete

Represent level/progression, base attributes, equipment slots, active set tiers, skills, buffs, pets, and morphs.

Separate build validation from derived-stat calculation. Item attribute requirements are displayed but not enforced on the final saved build because temporary effects and equipment swaps permit overequipping. Locally cast pets and attack skills still require the build's local level, progression, attributes, and sufficient Energy.

Deliverable: a character build that produces an inspectable stat breakdown.

Current status: `src/character.rs` defines serializable progression/allocation builds, typed combat equipment and implant slots, item/set/active-effect aggregation, final attributes, highest active set tiers with gathering-tool suppression, five shared buff/morph slots, and one Rebirth-or-later pet slot. Active effects contribute confirmed attributes, Armor, Attack Power, Crit, maximum resources, strongest-only attack speed, and typed periodic Energy regeneration. Locally cast skill requirements use base/allocation/item totals; set-granted attributes remain separate because whether they satisfy cast requirements is unconfirmed. Build inspection also evaluates all 40 direct-damage skill ranks and recommends the top three distinct castable skill names with Crit-aware burst DPS, resource-aware sustained DPS, full-Energy cast counts, and explicit model notes. Validation reports invalid allocations, missing/incompatible selections, slot limits, and confirmed requirement failures. `fo2-dps inspect-build <build.json>` loads the merged item/set/skill data and prints the inspection as JSON.

The initial Phase 4 deliverable is complete. Active buff/morph/pet effects are already applied to confirmed derived values. Remaining loadout questions include two-handed/off-hand exclusion and whether set-granted attributes satisfy local cast requirements; these stay explicit rather than being guessed.

## 5. First calculation milestone: one character versus one mob — initial milestone complete

Start narrowly with basic attacks and explicit assumptions, then add skills and resources.

Outputs should include:

- Expected hit damage and attack interval
- Expected outgoing and incoming DPS
- Time to kill
- Expected health lost
- Resource sustainability

Use analytical estimates as checks for an eventual discrete-event simulation.

Deliverable: a tested CLI encounter calculation with transparent inputs and assumptions.

Current status: `src/encounter.rs` provides tested analytical and expected-event basic-attack estimates. `fo2-dps encounter <build.json> <mob-slug> <assumptions.json>` validates the loadout, resolves the authoritative mob record, and prints source context, assumptions, analytical checks, and a timestamp-ordered expected-damage timeline. Build Lab derives expected noncritical basic-attack damage, Crit, attack interval, maximum Health, Dodge, and Armor; the web flow applies those values automatically. Mob damage uses the published midpoint and confirmed level/Armor mitigation unless an explicit post-mitigation override is supplied. The model applies chained Crit expectation, incoming Dodge, the opening player hit, and explicit simultaneous-event order. Outputs include attack counts, elapsed time, defeat outcome, remaining health, outgoing/incoming DPS, and expected health loss.

The initial Phase 5 deliverable is complete. This is not a random-roll simulator: it advances expected damage at discrete timestamps. Build recommendations model active Energy ticks as a long-run expected rate, but exact skill rotations and timestamped periodic Health/Energy ticks are not yet part of encounters. Mob debuffs, absorption, movement, latency, and scenario ranges remain later refinements.

## 6. Grinding and economy comparisons — initial milestone complete

After encounter calculations are trustworthy, add:

- Kills/hour with recovery, travel, and spawn assumptions
- Expected coins and item quantities per kill
- Shop value versus observed market-price scenarios
- Consumable costs
- Location-specific profitability

The web leaderboard uses shop values only because repeated farming would oversaturate the small market for non-consumable drops. Regular mobs have a confirmed 30-second respawn. A selected drop profile can be capped at `published map spawns × 120 kills/hour`; this is automatic for a zero-second opening-one-shot cycle and optional for positive-duration routes. Practical route wait remains a separate user input.

Deliverable: theoretical and practical mob/location comparisons with uncertainty made visible.

Current status: `src/grinding.rs` and `fo2-dps grind <build.json> <mob-slug> <assumptions.json>` extend the expected-event encounter result with an explicitly selected location-specific drop profile. Cycle time and kills/hour use the encounter timeline plus user-supplied travel, recovery, and respawn waits. Opening one-shots reset attack timing; when total cycle time is zero, the selected profile's published spawn count and confirmed 30-second regular-mob respawn automatically produce the finite `spawn count × 120 kills/hour` throughput. The same spawn cap remains optional for positive-duration routes. Coin expectation uses a named midpoint policy. Independent drop rolls remain separate, unknown chances propagate to unknown expectations, and each successful roll yields one item. The web leaderboard is shop-only; explicit single-route and CLI scenarios can still select market observations. Bosses remain excluded because their confirmed 30-minute respawn is not useful for sustained grinding. Explicit consumable quantities and unit costs produce gross and net value per kill/hour.

`fo2-dps grind-compare <build.json> <comparison.json>` evaluates multiple mob/location scenarios with independent encounter, downtime, quantity, price, and consumable assumptions. It can rank kills, coins, item value, gross value, or net value per hour. Failed scenarios and unknown selected metrics remain unranked with their errors/nulls preserved rather than being assigned zero. The initial Phase 6 deliverable is complete. Future refinements may add uncertainty ranges, price liquidity/taxes, inventory constraints, competition, and richer recovery models.

## 7. Web application and guided UX — in progress

Expose the validated data and calculators through a responsive local web application without changing the explicit-assumption policy.

Initial deliverables:

- Search and inspect items, mobs, skills, and item sets
- Create and validate character builds
- Run encounter and grinding scenarios
- Compare grinding targets
- Preserve calculator inputs locally and show raw assumptions/results
- Accessible desktop and mobile interaction

Current status: `src/web.rs` implements a dependency-free local HTTP server with embedded frontend assets and bounded request bodies. Compact APIs expose items, mobs, merged skills, sets, factions, build inspection, encounters, and grinding calculations. The responsive frontend provides searchable data exploration; artwork-backed equipment, buff/morph, and pet pickers with top-layer tooltips; a visual equipment/implant/outfit board; live build validation; active-set summaries; confirmed derived stats; in-combat Energy regeneration; and top-three castable attack-skill recommendations with burst/sustained DPS. Encounter and Grinding reuse the current Build Lab automatically. Grinding supports faction-scoped rankings, shop-only sustainable gold valuation, detailed coin/item/shop handling breakdowns, saved preferences, attack-reset-aware one-shot cycles, optional loot-click limits, and published spawn-throughput caps that are automatic for zero-second cycles and optional otherwise. Inputs persist locally, advanced JSON remains available for inspection/import/export, and frontend/backend build IDs detect stale running servers. Launch the native app with `fo2-dps serve [address]`; the safe default is `127.0.0.1:8787`. A serverless GitHub Pages build is also implemented: the calculator core compiles to WebAssembly, the existing route contracts dispatch in-browser, and `build-web-bundle` combines the normalized snapshots without destructively compacting mobs. The current schema-v2 bundle is about 4.17 MiB, and Pages never ships the separate raw mob archive. `scripts/build-pages.sh` assembles the no-billing `dist/` artifact, while `.github/workflows/pages.yml` validates and deploys it through GitHub Pages.

Next priorities:

1. Rescan the public database and rerun canonical/supplemental teachable-skill coverage.
2. Add exact timestamped periodic Health/Energy ticks and active-skill rotations to encounters; keep the current long-run Energy-rate recommendation model as the transparent approximation.
3. Decide and implement two-handed/off-hand compatibility once the game rule is confirmed.
4. Add saved named builds/scenarios and improve route-comparison visualization.
5. Audit narrow responsive layouts and continue replacing raw JSON-first detail views with task-specific summaries.
