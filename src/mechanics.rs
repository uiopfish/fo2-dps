//! Small, explicitly sourced mechanics primitives.
#![allow(dead_code)] // Consumed by the forthcoming build/encounter models.
//!
//! This module must not become a home for guessed combat formulas. Unknown rules
//! belong in `docs/mechanics.md` until evidence or a named scenario supplies them.

use crate::db::ItemSetBonus;

pub const BASE_ATTRIBUTE_VALUE: u32 = 20;
pub const PLAYER_SKILL_REQUEST_INTERVAL_SECONDS: f32 = 1.0;
pub const MAX_CAST_TIME_REDUCTION_PERCENT: u32 = 100;
pub const CAST_TIME_REDUCTION_FUNCTIONAL_IN_CURRENT_GAME: bool = false;
pub const PLAYER_BASIC_ATTACK_CAN_MISS: bool = false;
pub const PLAYER_ENGAGES_WITH_FIRST_HIT: bool = true;
pub const ATTACK_POWER_AFFECTS_SKILLS: bool = false;
pub const UNARMED_ATTACK_INTERVAL_SECONDS: f64 = 1.4;
pub const BASE_ARMOR: i32 = 0;
pub const BASE_ATTACK_POWER: i32 = 40;
pub const BASE_UNARMED_DAMAGE_MIN: u32 = 7;
pub const BASE_UNARMED_DAMAGE_MAX: u32 = 10;
pub const BASE_CRIT_DISPLAY_PERCENT: f64 = 6.43;
pub const BASE_DODGE_DISPLAY_PERCENT: f64 = 5.0;
pub const CRIT_ATTRIBUTE_POINTS_PER_PERCENT: f64 = 14.0;
pub const CRIT_DIMINISHING_RETURNS_THRESHOLD_PERCENT: f64 = 80.0;
pub const CRIT_ADDITION_ABOVE_THRESHOLD_MULTIPLIER: f64 = 0.5;
pub const UNARMED_INTRINSIC_DAMAGE_MIN: u32 = 3;
pub const UNARMED_INTRINSIC_DAMAGE_MAX: u32 = 6;
pub const OUT_OF_COMBAT_REGEN_TICK_SECONDS: u32 = 2;

pub fn basic_attack_power_damage_contribution(
    attack_power: i64,
    attack_interval_seconds: f64,
) -> Option<i64> {
    if attack_power < 0 || !attack_interval_seconds.is_finite() || attack_interval_seconds <= 0.0 {
        return None;
    }
    let contribution = (attack_power as f64 * attack_interval_seconds / 14.0).floor();
    (contribution <= i64::MAX as f64).then_some(contribution as i64)
}

pub fn unarmed_damage_from_attack_power(attack_power: i32) -> Option<(u32, u32)> {
    let contribution = basic_attack_power_damage_contribution(
        i64::from(attack_power),
        UNARMED_ATTACK_INTERVAL_SECONDS,
    )?;
    Some((
        u32::try_from(i64::from(UNARMED_INTRINSIC_DAMAGE_MIN) + contribution).ok()?,
        u32::try_from(i64::from(UNARMED_INTRINSIC_DAMAGE_MAX) + contribution).ok()?,
    ))
}

/// Applies a Crit addition, with additions above 80% contributing at half value.
pub fn crit_after_addition(current_percent: f64, addition_percent: f64) -> Option<f64> {
    if !current_percent.is_finite()
        || !addition_percent.is_finite()
        || current_percent < 0.0
        || addition_percent < 0.0
    {
        return None;
    }

    let full_value_room = (CRIT_DIMINISHING_RETURNS_THRESHOLD_PERCENT - current_percent).max(0.0);
    let full_value_addition = addition_percent.min(full_value_room);
    let diminished_addition = addition_percent - full_value_addition;
    Some(
        current_percent
            + full_value_addition
            + diminished_addition * CRIT_ADDITION_ABOVE_THRESHOLD_MULTIPLIER,
    )
}

pub fn displayed_crit_from_total_attributes(
    total_agility: u32,
    total_intellect: u32,
    direct_crit: u32,
) -> f64 {
    let attribute_points = total_agility.saturating_sub(BASE_ATTRIBUTE_VALUE)
        + total_intellect.saturating_sub(BASE_ATTRIBUTE_VALUE);
    let addition =
        f64::from(attribute_points) / CRIT_ATTRIBUTE_POINTS_PER_PERCENT + f64::from(direct_crit);
    crit_after_addition(BASE_CRIT_DISPLAY_PERCENT, addition)
        .expect("attribute and direct Crit additions are nonnegative and finite")
}

/// Confirmed only through the linear region ending at 40% Dodge.
pub fn dodge_in_confirmed_linear_region(total_agility: u32) -> Option<f64> {
    let points_above_base = total_agility.saturating_sub(BASE_ATTRIBUTE_VALUE);
    let dodge = BASE_DODGE_DISPLAY_PERCENT + f64::from(points_above_base) * 0.25;
    (dodge <= 40.0).then_some(dodge)
}

/// Bounded piecewise candidate matching both the controlled linear observation
/// and the recorded 873-Agility / 81.46% geared panel.
pub fn displayed_dodge_candidate(total_agility: u32) -> f64 {
    let mut points = total_agility.saturating_sub(BASE_ATTRIBUTE_VALUE);
    let first = points.min(140);
    points -= first;
    let second = points.min(160);
    points -= second;
    let third = points.min(320);
    points -= third;
    BASE_DODGE_DISPLAY_PERCENT
        + f64::from(first) * 0.25
        + f64::from(second) * 0.125
        + f64::from(third) * 0.0625
        + f64::from(points) * 0.00625
}

/// Bounded candidate matching the observed level-38, level-39, and level-104
/// nude panels. This is out-of-combat regeneration per two-second tick.
pub fn base_out_of_combat_health_regen_candidate(level: u32) -> u32 {
    level / 2 + 5
}

/// Bounded candidate matching the observed level-38, level-39, and level-104
/// nude panels. This is out-of-combat regeneration per two-second tick.
pub fn base_out_of_combat_energy_regen_candidate(level: u32) -> u32 {
    level / 2 + 10
}

pub fn out_of_combat_health_regen_candidate(
    level: u32,
    allocated_stamina: u32,
    allocated_strength: u32,
) -> f64 {
    f64::from(base_out_of_combat_health_regen_candidate(level))
        + (f64::from(allocated_stamina) + f64::from(allocated_strength)) * 0.1
}

/// The +0.1 Energy Regen per allocated Intellect term is currently based on
/// owner recollection and remains bounded until a controlled panel comparison.
pub fn out_of_combat_energy_regen_candidate(level: u32, allocated_intellect: u32) -> f64 {
    f64::from(base_out_of_combat_energy_regen_candidate(level))
        + f64::from(allocated_intellect) * 0.1
}

pub fn base_health(level: u32) -> Option<u32> {
    level.checked_mul(18)
}

pub fn base_energy(level: u32) -> Option<u32> {
    level.checked_mul(20)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterPanelObservation {
    pub level: u32,
    pub unspent_points: u32,
    pub health: u32,
    pub energy: u32,
    pub health_regen: Option<u32>,
    pub energy_regen: Option<u32>,
    pub armor: i32,
    pub damage_min: u32,
    pub damage_max: u32,
    pub attack_power: i32,
    pub displayed_attack_speed_seconds: f64,
    pub crit_percent: f64,
    pub dodge_percent: f64,
    pub stamina: u32,
    pub strength: u32,
    pub agility: u32,
    pub intellect: u32,
}

/// Nude, unbuffed Spawn screenshots supplied by the project owner, including
/// all-20 baselines and single-attribute allocations. These fixtures establish
/// current displayed values; Crit and Dodge are panel-rounded values.
pub const BASELINE_PANEL_OBSERVATIONS: [CharacterPanelObservation; 4] = [
    CharacterPanelObservation {
        level: 1,
        unspent_points: 2,
        health: 18,
        energy: 20,
        health_regen: None,
        energy_regen: None,
        armor: 0,
        damage_min: 7,
        damage_max: 10,
        attack_power: 40,
        displayed_attack_speed_seconds: 1.4,
        crit_percent: 6.43,
        dodge_percent: 5.0,
        stamina: 20,
        strength: 20,
        agility: 20,
        intellect: 20,
    },
    CharacterPanelObservation {
        level: 38,
        unspent_points: 76,
        health: 684,
        energy: 760,
        health_regen: Some(24),
        energy_regen: Some(29),
        armor: 0,
        damage_min: 7,
        damage_max: 10,
        attack_power: 40,
        displayed_attack_speed_seconds: 1.4,
        crit_percent: 6.43,
        dodge_percent: 5.0,
        stamina: 20,
        strength: 20,
        agility: 20,
        intellect: 20,
    },
    CharacterPanelObservation {
        level: 60,
        unspent_points: 0,
        health: 1080,
        energy: 1200,
        health_regen: None,
        energy_regen: None,
        armor: 600,
        damage_min: 43,
        damage_max: 46,
        attack_power: 400,
        displayed_attack_speed_seconds: 1.4,
        crit_percent: 6.43,
        dodge_percent: 5.0,
        stamina: 20,
        strength: 140,
        agility: 20,
        intellect: 20,
    },
    CharacterPanelObservation {
        level: 39,
        unspent_points: 0,
        health: 702,
        energy: 780,
        health_regen: Some(24),
        energy_regen: Some(29),
        armor: 0,
        damage_min: 22,
        damage_max: 25,
        attack_power: 196,
        displayed_attack_speed_seconds: 1.4,
        crit_percent: 12.0,
        dodge_percent: 24.5,
        stamina: 20,
        strength: 20,
        agility: 98,
        intellect: 20,
    },
];

/// Geared level-103 Ascension screenshot supplied by the project owner. Because
/// item contributions are not isolated, this constrains aggregate mechanics but
/// must not be used to infer per-point attribute formulas by itself.
pub const GEARED_LEVEL_103_ASCENSION_OBSERVATION: CharacterPanelObservation =
    CharacterPanelObservation {
        level: 103,
        unspent_points: 0,
        health: 18_434,
        energy: 8_875,
        health_regen: None,
        energy_regen: None,
        armor: 124_375,
        damage_min: 2_184,
        damage_max: 2_264,
        attack_power: 2_481,
        displayed_attack_speed_seconds: 2.0,
        crit_percent: 108.5,
        dodge_percent: 81.46,
        stamina: 474,
        strength: 315,
        agility: 873,
        intellect: 141,
    };

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponPanelObservation {
    pub item_slug: &'static str,
    pub stored_weapon_damage_min: u32,
    pub stored_weapon_damage_max: u32,
    pub stored_attack_interval_seconds: f64,
    pub attack_power: i32,
    pub panel_damage_min: u32,
    pub panel_damage_max: u32,
}

/// Nude, all-20 level-38 Spawn screenshots with only the named weapon equipped.
pub const LEVEL_38_TOY_WEAPON_OBSERVATIONS: [WeaponPanelObservation; 2] = [
    WeaponPanelObservation {
        item_slug: "toy-sword-26",
        stored_weapon_damage_min: 4,
        stored_weapon_damage_max: 8,
        stored_attack_interval_seconds: 1.6,
        attack_power: 40,
        panel_damage_min: 8,
        panel_damage_max: 12,
    },
    WeaponPanelObservation {
        item_slug: "toy-wand-2345",
        stored_weapon_damage_min: 2,
        stored_weapon_damage_max: 10,
        stored_attack_interval_seconds: 2.0,
        attack_power: 40,
        panel_damage_min: 7,
        panel_damage_max: 15,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Progression {
    Spawn,
    Rebirth,
    Ascension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribute {
    Stamina,
    Strength,
    Agility,
    Intellect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerPointEffect {
    pub attack_power: u32,
    pub max_health: u32,
    pub health_regen: f64,
    pub armor: u32,
    pub crit_chance_percentage_points: f64,
    pub max_energy: u32,
}

/// Effects of spending one point in an attribute, as supplied from the
/// creator's official Discord. The caller supplies `is_highest_allocated`;
/// tie behavior is intentionally not guessed here.
pub fn per_allocated_point(attribute: Attribute, is_highest_allocated: bool) -> PerPointEffect {
    let mut effect = PerPointEffect {
        attack_power: 0,
        max_health: 0,
        health_regen: 0.0,
        armor: 0,
        crit_chance_percentage_points: 0.0,
        max_energy: 0,
    };
    match attribute {
        Attribute::Stamina => {
            effect.attack_power = u32::from(is_highest_allocated) * 2;
            effect.max_health = 20;
            effect.health_regen = 0.1;
        }
        Attribute::Strength => {
            effect.attack_power = if is_highest_allocated { 3 } else { 1 };
            effect.armor = 5;
            effect.health_regen = 0.1;
        }
        Attribute::Agility => {
            effect.attack_power = u32::from(is_highest_allocated) * 2;
            effect.crit_chance_percentage_points = 0.075;
        }
        Attribute::Intellect => {
            effect.attack_power = u32::from(is_highest_allocated) * 2;
            effect.crit_chance_percentage_points = 0.075;
            effect.max_energy = 15;
        }
    }
    effect
}

/// Exact critical multiplier procedure, separated from random-number
/// generation so simulations can supply game-compatible rolls in [0, 100).
pub fn crit_multiplier_from_rolls(
    mut crit_chance_percent: f64,
    rolls_percent: impl IntoIterator<Item = f64>,
) -> Option<u32> {
    if !crit_chance_percent.is_finite() || crit_chance_percent < 0.0 {
        return None;
    }
    let mut multiplier = 1u32;
    let mut rolls = rolls_percent.into_iter();
    while crit_chance_percent > 0.0 {
        let roll = rolls.next()?;
        if !roll.is_finite() || !(0.0..100.0).contains(&roll) {
            return None;
        }
        let roll_chance = crit_chance_percent.min(90.0);
        if roll <= roll_chance {
            multiplier = multiplier.checked_add(1)?;
            crit_chance_percent -= 90.0;
        } else {
            break;
        }
    }
    Some(multiplier)
}

/// Expected multiplier under ideal percentage probabilities implied by the
/// chained loop. Exact Java `nextFloat` discretization is not modeled here.
pub fn expected_crit_multiplier(mut crit_chance_percent: f64) -> Option<f64> {
    if !crit_chance_percent.is_finite() || crit_chance_percent < 0.0 {
        return None;
    }
    let mut expected = 1.0;
    let mut reach_probability = 1.0;
    while crit_chance_percent > 0.0 {
        reach_probability *= crit_chance_percent.min(90.0) / 100.0;
        expected += reach_probability;
        crit_chance_percent -= 90.0;
    }
    Some(expected)
}

/// Weapon attack speed is seconds per attack. Speed buffs reduce that interval
/// in milliseconds and do not stack, so only the strongest reduction applies.
pub fn attack_interval_with_strongest_buff(
    weapon_interval_seconds: f64,
    buff_reductions_ms: &[u32],
) -> Option<f64> {
    if !weapon_interval_seconds.is_finite() || weapon_interval_seconds <= 0.0 {
        return None;
    }
    let strongest_seconds =
        f64::from(buff_reductions_ms.iter().copied().max().unwrap_or(0)) / 1000.0;
    let interval = weapon_interval_seconds - strongest_seconds;
    (interval > 0.0).then_some(interval)
}

/// Published attribute-point allocation budget for a displayed level.
///
/// Ascension follows the Rebirth rule and receives 20 extra points for every
/// displayed level above 100. Checked arithmetic avoids converting overflow
/// into a plausible-looking build budget.
pub fn attribute_allocation_budget(level: u32, progression: Progression) -> Option<u32> {
    let base = level.checked_mul(2)?;
    match progression {
        Progression::Spawn => Some(base),
        Progression::Rebirth => base.checked_add(level / 4),
        Progression::Ascension => base
            .checked_add(level / 4)?
            .checked_add(level.saturating_sub(100).checked_mul(20)?),
    }
}

/// Returns the one active tier under the published highest-eligible-tier rule.
/// Gathering tools suppress the entire combat set bonus.
pub fn active_set_tier(
    bonuses: &[ItemSetBonus],
    equipped_pieces: u32,
    gathering_tool_in_main_hand: bool,
) -> Option<&ItemSetBonus> {
    if gathering_tool_in_main_hand {
        return None;
    }

    bonuses
        .iter()
        .filter(|bonus| bonus.required_pieces <= equipped_pieces)
        .max_by_key(|bonus| bonus.required_pieces)
}

/// Applies the published floor-to-whole-number operation to an already-derived
/// nonnegative result. This does not decide where in a combat formula flooring
/// occurs, nor does it define range endpoint behavior.
pub fn floor_nonnegative_result(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0.0 || value > u64::MAX as f64 {
        return None;
    }
    Some(value.floor() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ItemSetBonus, ItemSetBonusEffect};

    fn tier(pieces: u32) -> ItemSetBonus {
        ItemSetBonus {
            required_pieces: pieces,
            effects: vec![ItemSetBonusEffect {
                stat: "Armor".into(),
                value: format!("+{pieces}"),
            }],
        }
    }

    #[test]
    fn attack_power_damage_contribution_matches_all_weapon_observations() {
        assert_eq!(basic_attack_power_damage_contribution(40, 1.6), Some(4));
        assert_eq!(basic_attack_power_damage_contribution(40, 2.0), Some(5));
        assert_eq!(
            basic_attack_power_damage_contribution(2_481, 2.0),
            Some(354)
        );
        assert_eq!(basic_attack_power_damage_contribution(40, 1.4), Some(4));
    }

    #[test]
    fn observed_attribute_crit_formula_and_direct_crit_share_the_eighty_percent_soft_cap() {
        let expected = 108.500_714_285_714_28;
        assert!((displayed_crit_from_total_attributes(873, 141, 61) - expected).abs() < 1e-9);
    }

    #[test]
    fn dodge_candidate_matches_controlled_and_geared_panels() {
        assert_eq!(displayed_dodge_candidate(20), 5.0);
        assert_eq!(displayed_dodge_candidate(98), 24.5);
        assert!((displayed_dodge_candidate(873) - 81.45625).abs() < f64::EPSILON);
        assert_eq!(dodge_in_confirmed_linear_region(98), Some(24.5));
        assert_eq!(dodge_in_confirmed_linear_region(873), None);
    }

    #[test]
    fn baseline_screenshots_are_recorded_without_hidden_extrapolation() {
        let [level_one, level_38, level_60_strength, level_39_agility] =
            BASELINE_PANEL_OBSERVATIONS;
        assert_eq!(
            level_one.unspent_points,
            attribute_allocation_budget(1, Progression::Spawn).unwrap()
        );
        assert_eq!(
            level_38.unspent_points,
            attribute_allocation_budget(38, Progression::Spawn).unwrap()
        );
        assert_eq!(base_health(38), Some(level_38.health));
        assert_eq!(base_energy(38), Some(level_38.energy));
        assert_eq!(level_one.damage_min, level_38.damage_min);
        assert_eq!(level_one.attack_power, level_38.attack_power);
        assert_eq!(
            level_one.displayed_attack_speed_seconds,
            UNARMED_ATTACK_INTERVAL_SECONDS
        );
        assert_eq!(
            level_38.displayed_attack_speed_seconds,
            UNARMED_ATTACK_INTERVAL_SECONDS
        );
        assert_eq!(base_health(60), Some(level_60_strength.health));
        assert_eq!(base_energy(60), Some(level_60_strength.energy));
        assert_eq!(level_60_strength.strength - 20, 120);
        assert_eq!(level_60_strength.armor, 120 * 5);
        assert_eq!(
            level_60_strength.attack_power - level_one.attack_power,
            120 * 3
        );

        assert_eq!(base_health(39), Some(level_39_agility.health));
        assert_eq!(base_energy(39), Some(level_39_agility.energy));
        assert_eq!(level_39_agility.agility - BASE_ATTRIBUTE_VALUE, 78);
        assert_eq!(level_39_agility.attack_power, BASE_ATTACK_POWER + 78 * 2);
        assert_eq!(level_39_agility.dodge_percent, 24.5);
        assert!(
            (displayed_crit_from_total_attributes(98, 20, 0) - 12.001_428_571_428_571).abs() < 1e-9
        );
        assert_eq!(dodge_in_confirmed_linear_region(98), Some(24.5));
        assert_eq!(unarmed_damage_from_attack_power(196), Some((22, 25)));
        assert_eq!(unarmed_damage_from_attack_power(400), Some((43, 46)));
    }

    #[test]
    fn toy_weapon_panels_preserve_distinct_unexplained_damage_contributions() {
        let [sword, wand] = LEVEL_38_TOY_WEAPON_OBSERVATIONS;
        assert_eq!(sword.attack_power, wand.attack_power);
        assert_eq!(sword.panel_damage_min - sword.stored_weapon_damage_min, 4);
        assert_eq!(sword.panel_damage_max - sword.stored_weapon_damage_max, 4);
        assert_eq!(wand.panel_damage_min - wand.stored_weapon_damage_min, 5);
        assert_eq!(wand.panel_damage_max - wand.stored_weapon_damage_max, 5);
        assert!(wand.stored_attack_interval_seconds > sword.stored_attack_interval_seconds);
    }

    #[test]
    fn geared_ascension_observation_does_not_imply_an_allocation_split() {
        let observation = GEARED_LEVEL_103_ASCENSION_OBSERVATION;
        assert_eq!(
            attribute_allocation_budget(observation.level, Progression::Ascension),
            Some(291)
        );
        assert!(observation.agility > BASE_ATTRIBUTE_VALUE + 291);
        assert!(observation.crit_percent > 100.0);
        assert_eq!(observation.dodge_percent, 81.46);
    }

    #[test]
    fn out_of_combat_regen_candidates_match_all_observed_nude_panels() {
        assert_eq!(base_out_of_combat_health_regen_candidate(38), 24);
        assert_eq!(base_out_of_combat_energy_regen_candidate(38), 29);
        assert_eq!(base_out_of_combat_health_regen_candidate(39), 24);
        assert_eq!(base_out_of_combat_energy_regen_candidate(39), 29);
        assert_eq!(base_out_of_combat_health_regen_candidate(104), 57);
        assert_eq!(base_out_of_combat_energy_regen_candidate(104), 62);
        assert_eq!(out_of_combat_health_regen_candidate(104, 10, 20), 60.0);
        assert_eq!(out_of_combat_energy_regen_candidate(104, 30), 65.0);
    }

    #[test]
    fn per_point_effects_do_not_assume_tie_behavior() {
        assert_eq!(
            per_allocated_point(Attribute::Strength, false).attack_power,
            1
        );
        assert_eq!(
            per_allocated_point(Attribute::Strength, true).attack_power,
            3
        );
        assert_eq!(
            per_allocated_point(Attribute::Stamina, false).max_health,
            20
        );
        assert_eq!(
            per_allocated_point(Attribute::Intellect, true).max_energy,
            15
        );
        assert_eq!(
            per_allocated_point(Attribute::Agility, false).crit_chance_percentage_points,
            0.075
        );
    }

    #[test]
    fn crit_additions_are_halved_only_above_eighty_percent() {
        assert_eq!(crit_after_addition(70.0, 5.0), Some(75.0));
        assert_eq!(crit_after_addition(78.0, 12.0), Some(85.0));
        assert_eq!(crit_after_addition(80.0, 12.0), Some(86.0));
        assert_eq!(crit_after_addition(90.0, 20.0), Some(100.0));
        assert_eq!(crit_after_addition(-1.0, 5.0), None);
        assert_eq!(crit_after_addition(50.0, f64::NAN), None);
    }

    #[test]
    fn creator_crit_loop_supports_chained_multipliers() {
        assert_eq!(crit_multiplier_from_rolls(0.0, []), Some(1));
        assert_eq!(crit_multiplier_from_rolls(50.0, [40.0]), Some(2));
        assert_eq!(crit_multiplier_from_rolls(50.0, [60.0]), Some(1));
        assert_eq!(
            crit_multiplier_from_rolls(200.0, [10.0, 80.0, 15.0]),
            Some(4)
        );
        assert_eq!(crit_multiplier_from_rolls(200.0, [10.0, 95.0]), Some(2));
        assert_eq!(expected_crit_multiplier(100.0), Some(1.99));
    }

    #[test]
    fn strongest_attack_speed_buff_is_the_only_one_applied() {
        assert!(
            (attack_interval_with_strongest_buff(1.6, &[100, 400]).unwrap() - 1.2).abs() < 1e-9
        );
        assert_eq!(attack_interval_with_strongest_buff(1.6, &[]), Some(1.6));
        assert_eq!(attack_interval_with_strongest_buff(0.4, &[400]), None);
    }

    #[test]
    fn published_attribute_budgets_are_distinct() {
        assert_eq!(
            attribute_allocation_budget(80, Progression::Spawn),
            Some(160)
        );
        assert_eq!(
            attribute_allocation_budget(80, Progression::Rebirth),
            Some(180)
        );
        assert_eq!(
            attribute_allocation_budget(80, Progression::Ascension),
            Some(180)
        );
        assert_eq!(
            attribute_allocation_budget(105, Progression::Ascension),
            Some(336)
        );
        assert_eq!(
            attribute_allocation_budget(u32::MAX, Progression::Spawn),
            None
        );
    }

    #[test]
    fn only_highest_eligible_set_tier_applies() {
        let bonuses = vec![tier(8), tier(4), tier(6)];
        assert_eq!(
            active_set_tier(&bonuses, 3, false).map(|x| x.required_pieces),
            None
        );
        assert_eq!(
            active_set_tier(&bonuses, 7, false).map(|x| x.required_pieces),
            Some(6)
        );
        assert_eq!(
            active_set_tier(&bonuses, 10, false).map(|x| x.required_pieces),
            Some(8)
        );
        assert!(active_set_tier(&bonuses, 10, true).is_none());
    }

    #[test]
    fn flooring_rejects_values_outside_its_documented_domain() {
        assert_eq!(floor_nonnegative_result(19.999), Some(19));
        assert_eq!(floor_nonnegative_result(20.0), Some(20));
        assert_eq!(floor_nonnegative_result(-0.1), None);
        assert_eq!(floor_nonnegative_result(f64::NAN), None);
    }
}
