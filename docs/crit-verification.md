# Crit Verification

Status date: 2026-10-05

This document records the Crit sources, current formula, and expected results for the observed level-103 Ascension build. A guildmate report supplied by the project owner gives the pre-soft-cap attribute formula as:

```text
Attribute Crit = (50 + Agility + Intellect) ÷ 14
```

Direct `+Crit` is added to that raw value before the shared 80% soft cap is applied.

## Table of contents

- [Observed build](#observed-build)
- [Current Crit rules](#current-crit-rules)
- [Why the numerator formula is used](#why-the-numerator-formula-is-used)
- [Detailed level-103 calculation](#detailed-level-103-calculation)
- [Expected critical-hit outcomes](#expected-critical-hit-outcomes)
- [Verification still needed](#verification-still-needed)

## Observed build

| Field | Value |
|---|---:|
| Level | 103 |
| Progression | Ascension |
| Agility | 873 |
| Intellect | 141 |
| Direct equipment Crit | +56 percentage points |
| Active-set Crit | +5 percentage points |
| Displayed Crit | 108.50% |
| Basic-attack damage | 2,184–2,264 |
| Expected noncritical hit | 2,224 |
| Attack interval | 2.00 seconds |

The regression fixture is `geared_level_103_panel_uses_total_attributes_for_confirmed_stats` in `src/character.rs`. The observed panel is also recorded as `GEARED_LEVEL_103_ASCENSION_OBSERVATION` in `src/mechanics.rs`.

The fixture represents the equipment as one aggregate item because the original observation did not isolate every equipped item's contribution. Aggregate equipment and set totals are known, but a verified per-item Crit list is not currently available. No active buffs, morphs, or pets are included.

## Current Crit rules

1. Calculate raw attribute Crit:

   ```text
   (50 + total Agility + total Intellect) ÷ 14
   ```

2. Add direct `+Crit` from equipment, the active set tier, buffs, and the pet.
3. Apply the shared soft cap to that complete raw total:

   ```text
   displayed Crit = raw Crit                         when raw Crit ≤ 80
   displayed Crit = 80 + (raw Crit − 80) ÷ 2       when raw Crit > 80
   ```

4. Crit is not capped at 100%. Values above 90 participate in chained critical hits.

The implementation is `displayed_crit_from_total_attributes()`, `crit_after_addition()`, and `expected_crit_multiplier()` in `src/mechanics.rs`.

## Why the numerator formula is used

The formula explains all three recorded panels without treating the rounded `6.43%` baseline as an exact internal value:

| Observation | Calculation before direct Crit | Result | In game |
|---|---:|---:|---:|
| All-20 baseline | `(50 + 20 + 20) ÷ 14` | `6.428571% → 6.43%` | 6.43% |
| Level 39, 98 Agility and 20 Intellect | `(50 + 98 + 20) ÷ 14` | `12.000000%` | 12.00% |
| Level-103 build | `(50 + 873 + 141) ÷ 14` | `76.000000%` | participates in 108.50% final result |

The earlier approximation was:

```text
6.43 + ((Agility − 20) + (Intellect − 20)) ÷ 14
```

It was extremely close, but it used the panel-rounded `6.43` as an exact base. It predicted `12.001429%` for the level-39 panel and `108.500714%` for the geared panel. The numerator formula instead produces both observed values exactly before panel rounding.

The nominal Discord description of `+0.075` percentage points per Agility or Intellect also does not fit the recorded level-39 panel. The active formula's marginal contribution is exactly `1 ÷ 14 = 0.07142857` percentage points per attribute point before the soft cap.

## Detailed level-103 calculation

### Attribute Crit

```text
Formula offset:    50
Agility:          873
Intellect:        141
────────────────────
Numerator:      1,064

1,064 ÷ 14 = 76% attribute Crit
```

The attribute totals can still be audited by source:

| Source | Agility | Intellect |
|---|---:|---:|
| Innate base | 20 | 20 |
| Allocated | 176 | 0 |
| Equipment | 647 | 121 |
| Active set tier | 30 | 0 |
| **Total** | **873** | **141** |

Unlike the previous approximation, the formula directly includes both innate attributes. The constant `50` is part of the numerator rather than a separately rounded base percentage.

### Direct Crit

| Source | Direct Crit |
|---|---:|
| Equipment | +56 percentage points |
| Active set tier | +5 percentage points |
| **Total** | **+61 percentage points** |

### Shared soft cap

Before diminishing returns:

```text
Attribute Crit: 76
Direct Crit:    61
─────────────────
Raw Crit:      137%
```

The first 80 percentage points apply at full value. The remaining 57 apply at half value:

```text
Displayed Crit
= 80 + (137 − 80) ÷ 2
= 80 + 28.5
= 108.5%
```

This exactly matches the recorded **108.50%** panel.

## Expected critical-hit outcomes

Fantasy Online 2 uses a chained critical-hit process:

1. Roll against up to 90 percentage points of remaining Crit.
2. A success adds one damage multiplier and subtracts 90 Crit.
3. Continue until a roll fails or no Crit remains.

At `108.5%` Crit:

- The first roll succeeds with 90% probability.
- After the first success, `18.5` Crit remains.
- The second roll succeeds conditionally with 18.5% probability.

| Damage multiplier | Probability |
|---|---:|
| ×1 | 10.00% |
| ×2 | `90% × 81.5% = 73.35%` |
| ×3 | `90% × 18.5% = 16.65%` |

Expected multiplier:

```text
1 + 0.90 + (0.90 × 0.185)
= 2.0665×
```

For an expected noncritical hit of 2,224:

```text
Expected damage per attack = 2,224 × 2.0665 = 4,595.896
Expected basic-attack DPS  = 4,595.896 ÷ 2 = 2,297.948
```

## Verification still needed

The numerator formula is the best current rule because it both matches the guildmate report and exactly explains every recorded Crit panel. Controlled tests would still strengthen it:

1. Add or remove known amounts of Agility below 80% Crit.
2. Add or remove known amounts of Intellect below 80% Crit.
3. Test values whose two-decimal displays distinguish `(50 + Agility + Intellect) ÷ 14` from nearby approximations.
4. Change direct Crit while preserving attributes immediately below, at, and above 80%.
5. Verify whether the game rounds only the final displayed result.
6. Preserve a complete per-item list for the level-103 build so `+56 Crit`, `+647 Agility`, and `+121 Intellect` can be audited independently.
