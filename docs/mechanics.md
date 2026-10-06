# Fantasy Online 2 Mechanics Specification

Status date: 2026-10-02. Sources are the public Fantasy Online 2 database. This document distinguishes published rules from unknown formulas; dataset correlations are not treated as mechanics.

## Table of contents

- [Confidence labels](#confidence-labels)
- [Confirmed rules](#confirmed-rules)
  - [Attributes and progression](#attributes-and-progression)
  - [Item-set activation](#item-set-activation)
  - [Skill casting](#skill-casting)
  - [Rolls and rounding](#rolls-and-rounding)
  - [Crit](#crit)
  - [Attack speed](#attack-speed)
  - [Health and energy effects](#health-and-energy-effects)
  - [Mobs](#mobs)
  - [Grinding MVP policies](#grinding-mvp-policies)
- [Recorded in-game observations](#recorded-in-game-observations)
  - [Nude all-20 Spawn baselines](#nude-all-20-spawn-baselines)
  - [Nude level-38 Toy weapon comparisons](#nude-level-38-toy-weapon-comparisons)
  - [Geared level-103 Ascension observation](#geared-level-103-ascension-observation)
- [Core unknown formulas blocking authoritative DPS/survivability](#core-unknown-formulas-blocking-authoritative-dpssurvivability)
- [First encounter-model policy](#first-encounter-model-policy)

## Confidence labels

- **Confirmed:** explicitly documented by the database or directly labeled source data.
- **Bounded:** part of the behavior is documented, but a conversion, ordering, or edge case is missing.
- **Unknown:** no authoritative rule has been found. Calculators must require an explicit scenario assumption or refuse the calculation.

## Confirmed rules

### Attributes and progression

Source: https://db.fantasyonline2.com/guides/builds-and-progression

- Agility, Strength, Stamina, and Intellect each begin at 20.
- Spawn allocation budget: `displayed level × 2`.
- Rebirth allocation budget: `displayed level × 2 + floor(displayed level ÷ 4)`.
- Ascension uses the Rebirth rule plus 20 points per level above 100.
- Item attribute requirements are checked when an item is initially equipped, but the game allows overequipping: temporary morphs, cross-buffs, and equipment swaps can satisfy the requirement, and the item remains equipped after those stats are lost. Saved-build validation must therefore display item requirements but must not require the final build to meet them.
- Buffs and morphs may be supplied by another player, so the receiving build does not need to meet their attribute requirements. Pets and attack skills are locally cast and do require the build to meet their attribute requirements.
- A character may have at most five active buff/morph effects and one active pet. Morphs use the same five slots as buffs. Pets are available only from Rebirth onward; Spawn builds cannot activate a pet.
- Level, progression, faction-notoriety, and guild-level restrictions are distinct from attribute overequipping and remain validated where published.

Combined Discord, guildmate, and panel evidence supports these effects per allocated point:

- Stamina: +2 Attack Power when it is the highest allocated attribute; +20 health; +0.1 health regeneration.
- Strength: +3 Attack Power when highest allocated, otherwise +1; +5 armor; +0.1 health regeneration.
- Agility: +2 Attack Power when highest allocated; `1 ÷ 14` percentage points of raw Crit before the soft cap; a separately described Dodge rule.
- Intellect: +2 Attack Power when highest allocated; +15 energy; `1 ÷ 14` percentage points of raw Crit before the soft cap.

The original Discord summary described the Crit increment as `0.075`; the exact numerator formula and observed panels instead give `1 ÷ 14 = 0.07142857`. Attack Power does not affect skills. Whether multiple attributes tied for highest all receive their highest bonus is not yet confirmed. The recorded level-103 equipped build confirms that item- and set-granted Stamina, Strength, and Agility participate in maximum Health, Armor, Attack Power, and Dodge calculations; these formulas use total attributes above the base 20 rather than allocated points alone. In-game observations confirm the linear Dodge contribution through 24.50%; the project owner has confirmed the piecewise high-Dodge rule documented below as the active calculator rule.

### Item-set activation

Sources:

- https://db.fantasyonline2.com/guides/item-sets-and-set-bonuses
- Individual pages under https://db.fantasyonline2.com/item-sets

- Only the highest eligible set tier contributes; lower eligible tiers are not cumulative.
- Reaching a later threshold replaces the earlier active tier.
- Each equipped copy counts as one piece.
- Combat set bonuses are inactive while a mining, unlocking, or wood-cutting tool is equipped in the main hand.

### Skill casting

Sources include https://db.fantasyonline2.com/skills/fireball-8 and other player skill pages.

- Displayed cast time is the base cast time.
- Cast-time reduction is capped at 100%.
- Player skills are limited to one cast request per second, including skills with no cooldown.
- Skill activation can depend on target, energy, cooldown, equipment, level, and attributes.
- Active skill damage uses the skill's published base values and is not scaled by attributes or Attack Power. Crit is the only player-stat scaling applied to active skill damage.
- Build recommendations calculate burst skill throughput as `range midpoint × expected chained-Crit multiplier ÷ max(base cast time, cooldown, 1-second cast-request interval)`. Resource-aware sustained DPS then uses the lower of that timing-limited cast rate and `active in-combat Energy regeneration per second ÷ Energy cost`. Active periodic ticks use their long-run expected rate; out-of-combat regeneration is excluded. Recommendations require local level/progression/attribute eligibility and enough maximum Energy for at least one cast, rank by sustained DPS then burst DPS, and report full-Energy cast count without inventing a fight duration.
- Moving can interrupt an action that requires the player to stand still.

Current-game observation supplied by the project owner: Cast Time Reduction does not work in game. The calculator must ignore that stat for the current ruleset rather than applying the published cap. Queueing and latency behavior remain outside the current model.

### Rolls and rounding

Sources include public player skill pages.

- Displayed minima and maxima are roll boundaries.
- Results are rounded down to a whole number.
- For player basic attacks, every integer in the displayed panel range is an equally likely inclusive result.
- Mobs do not reduce outgoing player basic-attack damage through defense or damage reduction.
- Skill endpoint/distribution behavior and modifier-floor ordering remain unknown.

### Crit

Sources:

- https://db.fantasyonline2.com/skills/l33t-skillz-6
- Creator-published code supplied from the official Discord by the project owner on 2026-09-15.
- Guildmate report supplied by the project owner on 2026-10-05 for the exact pre-soft-cap attribute numerator.

`Crit +2` is described as an extra 2% chance. Critical damage uses a chained multiplier algorithm: start at ×1; while Crit remains above zero, roll against `min(remaining Crit, 90)%`; each success adds ×1 and subtracts 90 Crit; the first failure ends the chain. This permits multipliers above ×2 when Crit exceeds 90. `src/mechanics.rs` preserves that algorithm and provides its exact expected multiplier.

Crit is uncapped. Attribute Crit and direct `+Crit` from every source are summed into one addition. Additions that remain at or below 80% apply at full value; the portion of the combined addition above 80% is halved. For example, applying a +12% Crit buff at 78% consumes 2 percentage points at full value to reach 80%, then contributes half of the remaining 10, producing 85% Crit. Starting at 80%, the same +12% buff produces 86% Crit. `src/mechanics.rs` models this piecewise addition separately from the chained critical-hit multiplier.

The active pre-soft-cap attribute formula is `(50 + total Agility + total Intellect) ÷ 14`. Direct `+Crit` then joins that raw total before the all-source soft cap is applied. This gives the all-20 baseline `(50 + 20 + 20) ÷ 14 = 6.428571%`, displayed as `6.43%`; the level-39 panel `(50 + 98 + 20) ÷ 14 = 12.00%` exactly; and the level-103 build `(50 + 873 + 141) ÷ 14 + 61 = 137%` raw, reduced to exactly `108.50%`. The earlier calculator approximation used the rounded `6.43%` panel baseline plus one point per 14 attribute points above the innate 20s, producing values only `0.001429` percentage points higher before the soft cap. The nominal `0.075` Discord description still conflicts with the observed exact marginal rate of `1 ÷ 14`; controlled verification remains useful. Crit is confirmed uncapped.

### Attack speed

Sources:

- Weapon item pages, e.g. https://db.fantasyonline2.com/items/magic-stick-17
- https://db.fantasyonline2.com/skills/rapid-fire-270
- Mob pages such as https://db.fantasyonline2.com/mobs/soft-shelled-crab-1

- Weapon attack speed is seconds per attack: a Toy Sword value of 1.6 means one attack every 1.6 seconds.
- Attack-speed buff values are millisecond reductions: Rapid Fire's `-400` makes the Toy Sword attack every 1.2 seconds.
- Attack-speed buffs do not stack; only the strongest buff applies.
- There is no separate player attack-speed stat beyond the equipped weapon and applicable strongest buff.
- Mob attack intervals are explicitly published in milliseconds and seconds.

Active buff, pet, and morph modifiers for Agility, Strength, Stamina, Intellect, Armor, Attack Power, Crit, Max Health, and Max Energy are aggregated into the character sheet. Attribute and direct Crit from active effects enter the same all-source 80% soft-cap calculation as equipment and set bonuses. Active attack-speed reductions use the strongest-only rule. Move speed, damage absorption, and the minimum possible player interval remain separate or unresolved mechanics. Periodic healing and energy are confirmed mechanics with published tick intervals, but are not yet scheduled by the encounter implementation.

Still unknown: minimum possible player interval and whether attack-speed debuffs use the same strongest-only selection rule. The player lands the first hit when engaging a mob.

### Health and energy effects

Regeneration has two distinct systems:

1. **Active effect regeneration:** Skills, buffs, and pets use the amount and tick interval published in their own descriptions. Their periodic regeneration remains active for the full duration that the pet or buff is active, including during combat. These effects must be scheduled independently rather than merged into passive regeneration.
2. **Out-of-combat regeneration:** The character panel's Health Regen and Energy Regen values proc every two seconds only while out of combat. The two-second timer continues counting down during combat, but a ready tick is suppressed rather than consumed. If combat lasts at least until the timer reaches zero, the pending regeneration procs immediately when the mob is killed and combat ends.

Allocated Stamina and Strength each add +0.1 Health Regen per point. The owner recalls an analogous Intellect contribution to Energy Regen, but its exact per-point value still needs a controlled allocation panel test. Each point of total Intellect above the base 20 adds the confirmed +15 maximum Energy, including equipment- and set-granted Intellect.

The nude level-38 and level-39 panels both show 24 Health Regen and 29 Energy Regen. The level-104 all-Agility playground panel shows 57 and 62, with no Stamina/Strength/Intellect allocation. All three observations exactly fit these **Bounded** base candidates per two-second out-of-combat tick:

```text
Base Health Regen = floor(level / 2) + 5
Base Energy Regen = floor(level / 2) + 10
```

Equipment regeneration units and their interaction with out-of-combat regeneration remain unknown.

### Mobs

Mob pages publish health, a damage range, and an attack interval. Some entities explicitly do not attack.

Player attacks do not use a hit/miss roll; player Dodge applies to incoming attacks. The player lands the first hit when engaging. Landed mob damage after Armor is:

```text
mob damage × (200 + player level × 50)
───────────────────────────────────────
(200 + player level × 50) + Armor
```

The encounter model uses the midpoint of the published mob-damage range as expected raw damage because the incoming roll distribution is not yet confirmed, then applies Dodge as the landed probability. An explicit post-mitigation hit can override this expectation. Negative Armor increases damage while the denominator remains positive; a zero or negative denominator is rejected rather than assigned invented behavior. Mob critical hits, penetration, hidden defenses, rounding of individual hits, and ordering with absorption remain unknown.

### Grinding MVP policies

- Every successful independent item roll produces one item and requires one loot click. Repeated rolls remain separate.
- Expected loot clicks/hour is the sum of known successful-roll probabilities per kill multiplied by kills/hour. If any listed roll probability is unknown, the total is unknown; an active click limit excludes that route.
- The web leaderboard uses shop value only. Repeated gold farming would oversaturate the game's small market for non-consumable drops, so market and recently-sold observations are not counted as sustainable leaderboard income. Explicit single-route and CLI scenarios retain those observation-based models for comparison.
- Mining veins, unlocking chests, and trees are excluded from the leaderboard through their typed required-tool metadata.
- Regular mobs respawn 30 seconds after death. Each selected drop profile's published map-spawn count gives `spawn count × 120 kills/hour`. This cap is optional for positive-duration routes and is the automatic finite bound when an opening one-shot plus zero route overhead produces a zero-second cycle. Effective route wait remains an independent input.
- Each normalized mob has an explicit `boss_candidate` boolean derived solely during collection from the archived source HTML's `achievement-boss-` marker. Candidates are excluded from the leaderboard. Bosses have a confirmed 30-minute respawn and are intentionally outside the sustained-grinding ranking; the marker-derived classification remains a bounded heuristic, not a confirmed mechanic.
- Unknown shop values contribute zero only to the leaderboard's known-value ranking subtotal; detailed route totals preserve them as unknown.

## Recorded in-game observations

### Nude all-20 Spawn baselines

Evidence: screenshots supplied by the project owner on 2026-09-15. Both equipment panels are empty, no active buff icons are visible, all attributes are 20, and all available points are unspent. The point totals identify Spawn progression (`level × 2`).

Level 1 values:

- Unspent points: 2
- Health: 18 / 18
- Energy: 20 / 20
- Armor: 0
- Damage: 7–10
- Attack Power: 40
- ATK Speed: 1.40 seconds per unarmed attack
- Critical: 6.43%
- Dodge: 5.00%

Level 38 values:

- Unspent points: 76
- Health: 684 / 684
- Energy: 760 / 760
- Health Regen: 24
- Energy Regen: 29
- Armor: 0
- Damage: 7–10
- Attack Power: 40
- ATK Speed: 1.40 seconds per unarmed attack
- Critical: 6.43%
- Dodge: 5.00%

Level 39, all 78 Spawn allocation points spent in Agility:

- Health: 702 / 702
- Energy: 780 / 780
- Health Regen: 24
- Energy Regen: 29
- Armor: 0
- Damage: 22–25
- Attack Power: 196
- ATK Speed: 1.40 seconds per unarmed attack
- Critical: 12.00%
- Dodge: 24.50%
- Agility: 98; Stamina, Strength, Intellect: 20

Level 60, all 120 Spawn allocation points spent in Strength:

- Health: 1,080 / 1,080
- Energy: 1,200 / 1,200
- Armor: 600
- Damage: 43–46
- Attack Power: 400
- ATK Speed: 1.40 seconds per unarmed attack
- Critical: 6.43%
- Dodge: 5.00%
- Strength: 140; Stamina, Agility, Intellect: 20

These screenshots establish the current nude, unallocated Spawn baseline:

- Health: `18 × displayed level`.
- Energy: `20 × displayed level`.
- Armor: 0.
- Unarmed damage: 7–10 before allocated-point bonuses.
- Attack Power: 40.
- Unarmed interval: 1.40 seconds.
- Displayed Crit: 6.43%.
- Displayed Dodge: 5.00% at base Agility 20.

The level-60 allocation independently confirms `120 × 5 = 600` Armor and baseline `40 + 120 × 3 = 400` Attack Power from highest-allocated Strength. Crit and Dodge remain unchanged because Agility and Intellect remain 20.

The regeneration observations produce `floor(38 / 2) + 5 = 24`, `floor(38 / 2) + 10 = 29`, and the same results at level 39. The level-104 playground values independently produce `floor(104 / 2) + 5 = 57` and `floor(104 / 2) + 10 = 62`. This is a strong exact fit across three panels, but remains Bounded until another controlled level or authoritative formula confirms it.

The level-39 allocation confirms baseline `40 + 78 × 2 = 196` Attack Power from highest-allocated Agility and the linear Dodge result `5.00% + 78 × 0.25% = 24.50%`. The linear Dodge rule is therefore confirmed through 24.50%; behavior near and above the stated 40% transition remains unknown. Its 12.00% Crit display exactly matches `(50 + 98 Agility + 20 Intellect) ÷ 14 = 12.00%`, rather than the nominal Discord description based on `0.075` percentage points per attribute point.

The three unarmed observations establish an intrinsic unarmed range of 3–6 plus `floor(total Attack Power ÷ 10)` at the tested Attack Power values:

- 40 Attack Power: `3–6 + 4 = 7–10`.
- 196 Attack Power: `3–6 + 19 = 22–25`.
- 400 Attack Power: `3–6 + 40 = 43–46`.

This is the unarmed specialization of the basic-attack panel formula confirmed below: at a 1.40-second interval, `floor(AP × 1.40 ÷ 14)` is identical to `floor(AP ÷ 10)`.

At Agility 20, the supplied rational expression gives:

`(20 / (20 + 500) + 0.0115) × 100 = 4.9961538…%`

The displayed 5.00% is consistent with rounding that value to two decimal places. This confirms the expression at one low-Agility point, but does not resolve the stated 40% transition, the post-transition `+0.125%` behavior, or whether gameplay uses the rounded display value.

The panel value is `ATK Speed: 1.40`; the decimal point is difficult to see in the screenshots. The project owner confirms it means one unarmed attack every 1.4 seconds, consistent with weapon attack-speed values being seconds per attack.

### Nude level-38 Toy weapon comparisons

Evidence: screenshots supplied by the project owner on 2026-09-16. The character remains level 38 with all attributes at 20, 76 unspent points, 40 Attack Power, no armor, and no equipment other than the named main-hand weapon.

Toy Sword (`toy-sword-26`):

- Stored database weapon range: 4–8
- Stored and displayed attack interval: 1.60 seconds
- Panel damage: 8–12
- Residual above the stored weapon endpoints: +4 to each endpoint

Toy Wand (`toy-wand-2345`):

- Stored database weapon range: 2–10
- Stored and displayed attack interval: 2.00 seconds
- Panel damage: 7–15
- Residual above the stored weapon endpoints: +5 to each endpoint

Neither item has Attack Power, flat Damage, attributes, or other combat stats in the stored record. Because the character has the same 40 Attack Power in both screenshots, a universal `weapon range + floor(AP ÷ 10)` rule would predict 8–12 for the sword but only 6–14 for the wand. The wand's actual 7–15 panel therefore disproves that universal weapon formula.

Together with the exact level-103 Pearlbreaker fixture, these observations establish the basic-attack panel equation:

`panel range = intrinsic/weapon range + floor(Attack Power × base attack interval seconds ÷ 14) + implant Damage`

The base attack interval is the unbuffed interval supplied by the equipped weapon, or 1.40 seconds when unarmed. Active attack-speed buffs change attack frequency but do not change damage per hit. The flat `Damage` stat in the current item snapshot is exclusive to implants.

This reproduces unarmed `floor(AP ÷ 10)`, Toy Sword 8–12, Toy Wand 7–15, and Pearlbreaker’s Glaive 2,184–2,264 exactly. Every integer in the displayed basic-attack range is equally likely with inclusive endpoints, and mobs do not reduce outgoing player damage.

### Geared level-103 Ascension observation

Evidence: screenshot supplied by the project owner on 2026-09-16. This character has equipment focused mostly on Dodge. The screenshot does not isolate item stats from allocated points, so it is an aggregate constraint rather than a per-point formula test.

- Level/progression: level 103 Ascension
- Unspent points: 0
- Health: 18,434 / 18,434
- Energy: 8,875 / 8,875
- Armor: 124,375
- Damage: 2,184–2,264
- Attack Power: 2,481
- ATK Speed: 2.00 seconds
- Critical: 108.50%
- Dodge: 81.46%
- Stamina: 474
- Strength: 315
- Agility: 873
- Intellect: 141

The published Ascension budget at level 103 is `103 × 2 + floor(103 ÷ 4) + 3 × 20 = 291` allocated points. The exact allocation is 115 Stamina, 0 Strength, 176 Agility, and 0 Intellect. The normalized equipment and active nine-piece Tiamat tier produce final totals of 474 Stamina, 315 Strength, 873 Agility, and 141 Intellect.

This fixture confirms three previously misapplied formulas:

- `Health = level × 18 + (total Stamina − 20) × 20 + direct Max Health = 18,434`.
- `Armor = direct item/set Armor + (total Strength − 20) × 5 = 124,375`.
- `Attack Power = 40 + (total Strength − 20) + 2 × (highest total attribute − 20) + direct item/set Attack Power = 2,481`.

The corrected panel reading of 8,875 confirms `Energy = level × 20 + (total Intellect − 20) × 15 + direct Max Energy`: `103 × 20 + 121 × 15 + 5,000 = 8,875`. The reported Crit numerator formula exactly explains the 108.50% panel: `(50 + 873 + 141) ÷ 14 + 61 = 137%` raw Crit, then `80 + (137 − 80) ÷ 2 = 108.50%`. Crit is confirmed uncapped.

The 873 Agility / 81.46% Dodge observation strongly fits the following piecewise candidate when all displayed Agility is treated identically:

- Base 20 Agility gives 5% Dodge.
- Each additional Agility adds 0.25 percentage points until 40% Dodge: 140 points, reaching 160 total Agility.
- The gain halves to 0.125 per Agility until 60%: 160 points, reaching 320 total Agility.
- The gain halves again to 0.0625 per Agility until 80%: 320 points, reaching 640 total Agility.
- Above 80%, the gain is 0.00625 per Agility—one tenth of the preceding rate: the remaining `873 - 640 = 233` points add `233 × 0.00625 = 1.45625`, producing 81.45625%, displayed as 81.46%.

Halving at 80% instead would make the final rate 0.03125 and predict 87.28%, so three successive halvings at 40%, 60%, and 80% do **not** match the screenshot. The project owner is confident in the documented piecewise rule, so it is now the active calculator rule; controlled boundary tests remain useful regression evidence rather than a prerequisite for use.

## Core unknown formulas blocking authoritative DPS/survivability

1. Confirmation of the bounded out-of-combat base-regeneration formulas, the exact allocated-Intellect Energy Regen contribution, and equipment-regeneration interaction. The two-second out-of-combat timer and pending post-combat tick behavior are confirmed.
2. Armor rounding on individual hits, any cap/floor beyond the positive-denominator domain, and ordering with absorption. The mitigation formula itself is active; `assume_player_survives` remains an explicit fallback that skips incoming damage.
3. Minimum player attack interval and detailed event ordering after the confirmed opening player hit.
4. Mob debuff trigger/cadence/stacking behavior.
5. Highest-final-attribute Attack Power behavior when two or more attributes tie.

## First encounter-model policy

Until those formulas are confirmed, production calculations must not silently invent them. A prototype may use named, user-selected scenarios, but must show every assumption and keep scenario results separate from confirmed game behavior.

The most valuable remaining measurements are:

- Record individual incoming hits from the same mob at two or more known Armor totals, ideally including zero and negative Armor, to verify range distribution, rounding, and any floor/cap around the confirmed mitigation formula. Use the explicit survival assumption only when intentionally bypassing incoming damage.
- Capture Crit before/after one known direct-Crit item on the level-103 build to verify the shared soft cap independently; the configured numerator formula predicts exactly 108.50%.
- Remove one known Agility item from the level-103 build and record final Agility and Dodge as regression evidence for the active piecewise rule.
- Create a build where two final attributes tie for highest and record Attack Power, then move one attribute ahead by one point.
- For regeneration, record the panel before/after one Stamina, Strength, or Intellect change separately for allocated and equipment-granted attributes.
- For attack-speed limits, record one weapon with increasingly strong speed effects and exact observed intervals.

Record exact equipment, active buffs/set bonuses, attributes, target, and at least 20–50 hit samples for random ranges. Do not use averages alone when endpoint and flooring behavior are under investigation.
