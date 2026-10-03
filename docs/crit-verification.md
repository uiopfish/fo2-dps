# Crit Verification

Status date: 2026-09-18

This document records the Crit sources, current formula, and expected results for the observed level-103 Ascension build. The calculator now uses the attribute formula that fits both available in-game Crit panels:

```text
Attribute Crit = ((Agility − 20) + (Intellect − 20)) ÷ 14
```

The subtraction is applied to each attribute independently with a minimum contribution of zero.

## Table of contents

- [Observed build](#observed-build)
- [Current Crit rules](#current-crit-rules)
- [Why division by 14 is used](#why-division-by-14-is-used)
- [Detailed Crit sources](#detailed-crit-sources)
  - [Agility](#agility)
  - [Intellect](#intellect)
  - [Combined attribute Crit](#combined-attribute-crit)
  - [Direct Crit](#direct-crit)
- [Expected displayed result](#expected-displayed-result)
- [Base-20 check](#base-20-check)
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

The fixture represents the equipment as one aggregate item because the original observation did not isolate every equipped item's contribution. Aggregate equipment and set totals are known, but a verified per-item Crit list is not currently available.

No active buffs, morphs, or pets are included in this recorded fixture.

## Current Crit rules

- Base displayed Crit is `6.43%` when Agility and Intellect are both at their innate value of 20.
- Innate base attributes do not grant additional Crit.
- Agility and Intellect above their respective base values are combined.
- Every 14 contributing attribute points grant 1 percentage point of Crit before the soft cap.
- Direct `+Crit` from equipment, active set tiers, buffs, and pets joins the same addition pool.
- Additions apply at full value until displayed Crit reaches `80%`.
- The portion of the combined addition above `80%` applies at half value.
- Crit is not capped at `100%`; values above 90 participate in chained critical hits.

The implementation is `displayed_crit_from_total_attributes()` and `crit_after_addition()` in `src/mechanics.rs`.

## Why division by 14 is used

The earlier calculator used the nominal Discord description of `0.075` percentage points per Agility or Intellect point. That produced values inconsistent with both available panels.

| Observation | Nominal `× 0.075` formula | Candidate `÷ 14` formula | In game |
|---|---:|---:|---:|
| Level 39, 98 Agility and 20 Intellect | 12.28% | 12.0014% → 12.00% | 12.00% |
| Level 103 build after shared soft cap | 110.24% | 108.5007% → 108.50% | 108.50% |

The `÷ 14` formula is therefore a substantially better fit. It is still an empirical formula based on two observations; controlled point-by-point tests remain useful.

## Detailed Crit sources

### Agility

The build's 873 total Agility consists of:

| Source | Agility | Crit before soft cap |
|---|---:|---:|
| Innate base | 20 | 0% |
| Allocated | 176 | `176 ÷ 14 = 12.571429%` |
| Equipment | 647 | `647 ÷ 14 = 46.214286%` |
| Active set tier | 30 | `30 ÷ 14 = 2.142857%` |
| **Contributing total** | **853** | **60.928571%** |
| **Displayed total Agility** | **873** | — |

Equivalent total-attribute calculation:

```text
(873 − 20) ÷ 14
= 853 ÷ 14
= 60.928571 percentage points
```

### Intellect

The build's 141 total Intellect consists of:

| Source | Intellect | Crit before soft cap |
|---|---:|---:|
| Innate base | 20 | 0% |
| Allocated | 0 | 0% |
| Equipment | 121 | `121 ÷ 14 = 8.642857%` |
| Active set tier | 0 | 0% |
| **Contributing total** | **121** | **8.642857%** |
| **Displayed total Intellect** | **141** | — |

Equivalent total-attribute calculation:

```text
(141 − 20) ÷ 14
= 121 ÷ 14
= 8.642857 percentage points
```

### Combined attribute Crit

```text
Contributing Agility:   853
Contributing Intellect: 121
───────────────────────────
Combined points:        974

974 ÷ 14 = 69.571429 percentage points
```

### Direct Crit

| Source | Direct Crit |
|---|---:|
| Equipment | +56 percentage points |
| Active set tier | +5 percentage points |
| **Total** | **+61 percentage points** |

## Expected displayed result

All additions after the base value are:

```text
Attribute Crit:  69.571429
Direct Crit:     61.000000
────────────────────────
Total addition: 130.571429 percentage points
```

Without diminishing returns:

```text
6.43 + 130.571429 = 137.001429%
```

Starting from `6.43%`, the full-value room before the `80%` threshold is:

```text
80 − 6.43 = 73.57 percentage points
```

The remaining addition above the threshold is:

```text
130.571429 − 73.57 = 57.001429
```

That remainder applies at half value:

```text
57.001429 ÷ 2 = 28.500714
```

Therefore:

```text
Expected displayed Crit
= 80 + 28.500714
= 108.500714%
```

Rounded to two decimal places, the expected panel value is **108.50%**, matching the screenshot.

## Base-20 check

The innate 20 points are removed from both attributes:

```text
Agility contribution points   = 873 − 20 = 853
Intellect contribution points = 141 − 20 = 121
```

The formula is therefore not `(873 + 141) ÷ 14`. It is:

```text
((873 − 20) + (141 − 20)) ÷ 14
= 974 ÷ 14
```

Including the two innate base values would incorrectly add `40 ÷ 14 = 2.857143` pre-cap percentage points, or approximately `1.428571` displayed points because this build is already above the soft cap.

## Expected critical-hit outcomes

Fantasy Online 2 uses a chained critical-hit process:

1. Roll against up to 90 percentage points of remaining Crit.
2. A success adds one damage multiplier and subtracts 90 Crit.
3. Continue until a roll fails or no Crit remains.

At `108.500714%` Crit:

- The first critical roll succeeds with 90% probability.
- After the first success, `18.500714` Crit remains.
- The second roll succeeds conditionally with `18.500714%` probability.

| Damage multiplier | Probability |
|---|---:|
| ×1 | 10.000000% |
| ×2 | `90% × 81.499286% = 73.349357%` |
| ×3 | `90% × 18.500714% = 16.650643%` |

Expected multiplier:

```text
1 + 0.90 + (0.90 × 0.18500714)
= 2.06650643×
```

For an expected noncritical hit of 2,224:

```text
2,224 × 2.06650643
= 4,595.91 expected damage per attack
```

At a two-second attack interval:

```text
4,595.91 ÷ 2
= 2,297.96 expected basic-attack DPS
```

## Verification still needed

The divisor-14 formula fits every currently recorded Crit panel, but the exact internal rule should still be tested by recording panel Crit after controlled changes:

1. Add or remove known amounts of Agility below 80% Crit.
2. Add or remove known amounts of Intellect below 80% Crit.
3. Test enough values to distinguish exact division by 14 from an approximation or hidden rounding rule.
4. Change direct Crit while preserving attributes immediately below, at, and above 80%.
5. Check whether the panel rounds only the final result or rounds intermediate attribute contributions.
6. Preserve the complete per-item equipment list for the level-103 build so `+56 Crit`, `+647 Agility`, and `+121 Intellect` can be audited item by item.

Until more controlled samples are available, `((Agility − 20) + (Intellect − 20)) ÷ 14` is the calculator's best-supported displayed-Crit formula.
