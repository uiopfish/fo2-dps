# Development Notes

## Table of contents

- [Resolved implementation notes](#resolved-implementation-notes)
- [For future development](#for-future-development)

## Resolved implementation notes

These notes were implemented on 2026-09-17 and moved into the canonical behavior documentation in `README.md` and `docs/mechanics.md`.

- Buff and morph attribute requirements are exempt for receiving builds because those effects may be supplied by another player.
- Pet and attack-skill attribute requirements remain enforced because the player casts them locally.
- Item attribute requirements are informational during saved-build validation because temporary stats and equipment swaps permit overequipping. Published level, progression, faction, and guild restrictions remain enforced.
- Grinding reuses safely derived values from the current Build Lab inspection through its internal combat-cycle engine. Unknown or out-of-domain mechanics remain explicit inputs, and route comparisons receive the same derived build values at submission.
- Grinding uses one item and one loot click per successful independent roll. Expected clicks/hour is available in leaderboard results, with an optional maximum-click filter.
- Grinding leaderboard item valuation is shop-only. Repeated farming would oversaturate the small market for non-consumable drops, so observed market sales are not treated as sustainable gold income. Explicit single-route and CLI scenarios may still inspect other price observations.
- Mining, unlocking, and wood-cutting resource targets are excluded from the leaderboard using typed required-weapon metadata.
- Mob storage is normalized. Tracked `data/mobs.json` contains 467 application records and no source HTML, raw tables, or raw cells. Bulk collection writes source evidence separately to the gitignored gzip JSONL `data/mobs.raw.jsonl.gz`, records both file checksums in tracked `data/mobs.provenance.json`, and also writes the scrape report. Native startup, SQLite import, and web-bundle generation read the normalized snapshot; Pages never ships the archive. The archive is not a documented release asset, so retain it separately when future reparsing matters and use the manifest to verify the exact copy.
- Boss candidates are represented by the explicit normalized `boss_candidate` boolean, derived solely from the archived `achievement-boss-` marker. They are excluded from the leaderboard because the confirmed 30-minute respawn allows at most two kills per hour per spawn and is not competitive for the intended grinding ranking; the classification remains heuristic.
- Build Lab provides five shared buff/morph picker slots and one ranked pet-picker slot. It enforces one pet from Rebirth onward, recommends attack skills separately, and applies typed active stat modifiers including all-source Crit soft-capping.

## For future development

- Rescan the public database and rerun the teaching-item completeness audit. The current snapshot still lacks canonical records for several taught teleports, utility skills, and later ranks; keep local supplemental records separate from scraped canonical data.
- Implement active skill rotations when they become a priority. Active skill damage has no attribute or Attack Power scaling; Crit is its only player-stat scaling. Preserve each skill's published base range, cast/cooldown/energy constraints, and the existing chained-Crit rule.
- Integrate published periodic Health and Energy effects into the discrete encounter timeline. Build recommendations already use periodic Energy ticks as a long-run expected Energy/second rate; exact encounter scheduling still needs tick timestamps. Pet and buff tick amounts/intervals are in the data, continue during combat, and use the same active-effect behavior; the only pet-specific slot rule is the one-pet limit.
- Regular mobs respawn 30 seconds after death. The selected drop profile's published map-spawn count gives `spawn count × 120 kills/hour`. The web leaderboard applies this automatically to zero-second opening-one-shot cycles and offers it as an opt-in cap for positive-duration routes; effective route wait and travel remain separate inputs.
- Continue collecting controlled panel observations for the still-bounded derived formulas called out in `docs/mechanics.md`. The basic-attack weapon formula is already confirmed for the current calculator scope and does not need to be reopened without contradictory evidence.
- Replace the normalized `boss_candidate` marker heuristic if authoritative boss/miniboss classification becomes available. Do not infer additional boss status from names or combat values. Bosses remain intentionally excluded even with the confirmed 30-minute respawn because they are not useful sustained-grinding targets.
