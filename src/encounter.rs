//! Transparent analytical encounter estimates.
//!
//! Confirmed outgoing damage and Armor mitigation are combined with timing, Crit,
//! Dodge, and opening-hit rules. Unresolved mechanics remain named scenario inputs.

use crate::mechanics::expected_crit_multiplier;
use crate::mobs::Mob;

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EncounterAssumptions {
    /// Explicit fallback that skips incoming damage and assumes survival.
    #[serde(default)]
    pub assume_player_survives: bool,
    /// Filled from Build Lab by application entry points.
    #[serde(default)]
    pub player_level: Option<u32>,
    /// Raw character-sheet Armor, filled from Build Lab.
    #[serde(default)]
    pub player_armor: Option<i64>,
    /// Determines which event resolves first when player and mob attack timestamps match.
    pub simultaneous_event_order: SimultaneousEventOrder,
    /// Expected landed noncritical damage from the inclusive panel range.
    pub expected_noncritical_player_hit: f64,
    /// Final displayed Crit chance after all additions and diminishing returns.
    pub player_crit_percent: f64,
    /// Final attack interval after the strongest applicable speed modifier.
    pub player_attack_interval_seconds: f64,
    pub player_max_health: f64,
    /// Final displayed Dodge chance used as the incoming miss probability.
    pub player_dodge_percent: f64,
    /// Optional expected landed mob-damage override after Armor mitigation.
    /// When omitted, attacking mobs use their published range midpoint and the
    /// build-derived level and Armor.
    pub expected_post_mitigation_mob_hit: Option<f64>,
    /// Optional resource scenario. Positive means net energy spent per second;
    /// negative means net regeneration.
    pub energy: Option<EnergyAssumptions>,
}

impl EncounterAssumptions {
    pub fn apply_build_defense(&mut self, level: u32, armor: i64) {
        self.player_level = Some(level);
        self.player_armor = Some(armor);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SimultaneousEventOrder {
    PlayerFirst,
    MobFirst,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EnergyAssumptions {
    pub initial_energy: f64,
    pub net_cost_per_second: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EnergyEstimate {
    pub initial_energy: f64,
    pub net_cost_per_second: f64,
    pub expected_encounter_cost: f64,
    pub expected_remaining_energy: f64,
    pub sustainable_for_expected_encounter: bool,
    pub sustainable_seconds: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EncounterOutcome {
    MobDefeated,
    PlayerDefeated,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ExpectedEventTimeline {
    pub outcome: EncounterOutcome,
    pub elapsed_seconds: f64,
    pub player_attacks: u64,
    pub mob_attacks: u64,
    pub expected_mob_health_remaining: f64,
    pub expected_player_health_remaining: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EncounterEstimate {
    pub mob_slug: String,
    pub mob_health: u64,
    pub expected_crit_multiplier: f64,
    pub expected_player_damage_per_attack: f64,
    pub player_attack_interval_seconds: f64,
    pub expected_outgoing_dps: f64,
    /// Continuous analytical estimate after an immediate opening player attack.
    pub expected_time_to_kill_seconds: f64,
    pub expected_mob_damage_per_landed_attack: f64,
    pub mob_attack_interval_seconds: Option<f64>,
    pub expected_incoming_dps: f64,
    pub expected_health_lost: f64,
    pub expected_remaining_health: f64,
    pub survives_in_expectation: bool,
    pub expected_event_timeline: ExpectedEventTimeline,
    pub energy: Option<EnergyEstimate>,
    pub assumptions: EncounterAssumptions,
    pub notes: Vec<String>,
}

pub fn estimate_encounter(
    mob: &Mob,
    assumptions: EncounterAssumptions,
) -> Result<EncounterEstimate, String> {
    positive_finite(
        assumptions.expected_noncritical_player_hit,
        "expected_noncritical_player_hit",
    )?;
    nonnegative_finite(assumptions.player_crit_percent, "player_crit_percent")?;
    positive_finite(
        assumptions.player_attack_interval_seconds,
        "player_attack_interval_seconds",
    )?;
    positive_finite(assumptions.player_max_health, "player_max_health")?;
    percentage(assumptions.player_dodge_percent, "player_dodge_percent")?;

    let mob_health = mob
        .health
        .ok_or_else(|| "mob health is unknown".to_string())?;
    if mob_health == 0 {
        return Err("mob health must be positive".into());
    }

    let crit_multiplier = expected_crit_multiplier(assumptions.player_crit_percent)
        .ok_or_else(|| "player Crit is outside the supported numeric domain".to_string())?;
    let expected_player_damage = assumptions.expected_noncritical_player_hit * crit_multiplier;
    let outgoing_dps = expected_player_damage / assumptions.player_attack_interval_seconds;
    let health_after_opening = (mob_health as f64 - expected_player_damage).max(0.0);
    let time_to_kill = health_after_opening / outgoing_dps;

    let (mob_hit, mob_interval, incoming_dps) = if assumptions.assume_player_survives {
        (0.0, None, 0.0)
    } else {
        match mob.attacks {
            Some(false) => (0.0, None, 0.0),
            Some(true) => {
                let hit = match assumptions.expected_post_mitigation_mob_hit {
                    Some(hit) => hit,
                    None => expected_mob_hit_after_armor(
                        mob,
                        assumptions.player_level.ok_or_else(|| {
                            "player level is required for Armor mitigation".to_string()
                        })?,
                        assumptions.player_armor.ok_or_else(|| {
                            "player Armor is required for Armor mitigation".to_string()
                        })?,
                    )?,
                };
                nonnegative_finite(hit, "expected_post_mitigation_mob_hit")?;
                let interval = mob
                    .attack_speed_ms
                    .filter(|interval| *interval > 0)
                    .ok_or_else(|| "attacking mob has no positive attack interval".to_string())?
                    as f64
                    / 1000.0;
                let landed_probability = 1.0 - assumptions.player_dodge_percent / 100.0;
                (hit, Some(interval), hit * landed_probability / interval)
            }
            None => return Err("whether this mob attacks is unknown".into()),
        }
    };

    let health_lost = incoming_dps * time_to_kill;
    let remaining_health = assumptions.player_max_health - health_lost;
    let timeline = expected_event_timeline(
        mob_health as f64,
        expected_player_damage,
        assumptions.player_attack_interval_seconds,
        assumptions.player_max_health,
        mob_hit * (1.0 - assumptions.player_dodge_percent / 100.0),
        mob_interval,
        assumptions.simultaneous_event_order,
    )?;
    let energy = assumptions
        .energy
        .as_ref()
        .map(|energy| energy_estimate(energy, timeline.elapsed_seconds))
        .transpose()?;

    let assumed_survival = assumptions.assume_player_survives;
    let has_mob_hit_override = assumptions.expected_post_mitigation_mob_hit.is_some();
    Ok(EncounterEstimate {
        mob_slug: mob.slug.clone(),
        mob_health,
        expected_crit_multiplier: crit_multiplier,
        expected_player_damage_per_attack: expected_player_damage,
        player_attack_interval_seconds: assumptions.player_attack_interval_seconds,
        expected_outgoing_dps: outgoing_dps,
        expected_time_to_kill_seconds: time_to_kill,
        expected_mob_damage_per_landed_attack: mob_hit,
        mob_attack_interval_seconds: mob_interval,
        expected_incoming_dps: incoming_dps,
        expected_health_lost: health_lost,
        expected_remaining_health: remaining_health,
        survives_in_expectation: timeline.outcome == EncounterOutcome::MobDefeated,
        expected_event_timeline: timeline,
        energy,
        assumptions,
        notes: vec![
            "Player basic-attack damage is uniformly distributed across the inclusive panel range, mobs have no outgoing damage reduction, and the player lands an immediate attack that cannot miss.".into(),
            if assumed_survival {
                "This scenario explicitly overrides incoming damage and assumes the player survives.".into()
            } else if has_mob_hit_override {
                "Expected landed mob damage uses the explicit scenario override.".into()
            } else {
                "Expected landed mob damage uses mob_damage × (200 + player_level × 50) ÷ ((200 + player_level × 50) + Armor).".into()
            },
            "DPS and continuous time-to-kill remain analytical cross-checks; expected_event_timeline resolves expected-damage events at explicit attack timestamps.".into(),
            "Mob debuffs, player skills, healing, regeneration ticks, absorption, movement, latency, and recovery are not modeled.".into(),
        ],
    })
}

pub fn expected_mob_hit_after_armor(
    mob: &Mob,
    player_level: u32,
    armor: i64,
) -> Result<f64, String> {
    let damage = mob
        .damage
        .as_ref()
        .ok_or_else(|| "mob damage range is unknown".to_string())?;
    let expected_raw_damage = (damage.min as f64 + damage.max as f64) / 2.0;
    let level_scale = 200.0 + f64::from(player_level) * 50.0;
    let denominator = level_scale + armor as f64;
    if !denominator.is_finite() || denominator <= 0.0 {
        return Err("Armor mitigation denominator must be positive".into());
    }
    Ok(expected_raw_damage * level_scale / denominator)
}

fn expected_event_timeline(
    mut mob_health: f64,
    player_damage: f64,
    player_interval: f64,
    mut player_health: f64,
    mob_damage: f64,
    mob_interval: Option<f64>,
    event_order: SimultaneousEventOrder,
) -> Result<ExpectedEventTimeline, String> {
    let mut player_attacks = 0u64;
    let mut mob_attacks = 0u64;
    let mut next_player = 0.0;
    let mut next_mob = mob_interval.unwrap_or(f64::INFINITY);

    loop {
        let simultaneous = (next_player - next_mob).abs() < 1e-9;
        let player_turn = next_player < next_mob
            || (simultaneous && event_order == SimultaneousEventOrder::PlayerFirst);
        if player_turn {
            player_attacks = player_attacks
                .checked_add(1)
                .ok_or_else(|| "player attack count overflow".to_string())?;
            mob_health -= player_damage;
            if mob_health <= 0.0 {
                return Ok(ExpectedEventTimeline {
                    outcome: EncounterOutcome::MobDefeated,
                    elapsed_seconds: next_player,
                    player_attacks,
                    mob_attacks,
                    expected_mob_health_remaining: mob_health.max(0.0),
                    expected_player_health_remaining: player_health.max(0.0),
                });
            }
            next_player += player_interval;
        } else {
            mob_attacks = mob_attacks
                .checked_add(1)
                .ok_or_else(|| "mob attack count overflow".to_string())?;
            player_health -= mob_damage;
            if player_health <= 0.0 {
                return Ok(ExpectedEventTimeline {
                    outcome: EncounterOutcome::PlayerDefeated,
                    elapsed_seconds: next_mob,
                    player_attacks,
                    mob_attacks,
                    expected_mob_health_remaining: mob_health.max(0.0),
                    expected_player_health_remaining: player_health.max(0.0),
                });
            }
            next_mob += mob_interval.expect("finite mob event has an interval");
        }
    }
}

fn energy_estimate(
    assumptions: &EnergyAssumptions,
    encounter_seconds: f64,
) -> Result<EnergyEstimate, String> {
    nonnegative_finite(assumptions.initial_energy, "initial_energy")?;
    if !assumptions.net_cost_per_second.is_finite() {
        return Err("net_cost_per_second must be finite".into());
    }
    let expected_cost = assumptions.net_cost_per_second * encounter_seconds;
    let remaining = assumptions.initial_energy - expected_cost;
    Ok(EnergyEstimate {
        initial_energy: assumptions.initial_energy,
        net_cost_per_second: assumptions.net_cost_per_second,
        expected_encounter_cost: expected_cost,
        expected_remaining_energy: remaining,
        sustainable_for_expected_encounter: remaining >= 0.0,
        sustainable_seconds: (assumptions.net_cost_per_second > 0.0)
            .then_some(assumptions.initial_energy / assumptions.net_cost_per_second),
    })
}

fn positive_finite(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be finite and positive"))
    }
}

fn nonnegative_finite(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be finite and nonnegative"))
    }
}

fn percentage(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() && (0.0..=100.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} must be between 0 and 100"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{MobRange, MobRawPage};

    fn mob(attacks: Option<bool>) -> Mob {
        Mob {
            slug: "target".into(),
            name: "Target".into(),
            source_url: String::new(),
            level: Some(1),
            health: Some(100),
            damage: None,
            attack_speed_ms: Some(2_000),
            attacks,
            faction: None,
            faction_xp: None,
            required_weapon: None,
            aggressive: None,
            debuff_skill_count: None,
            debuffs: Vec::new(),
            locations: Vec::new(),
            drop_profiles: Vec::new(),
            raw: MobRawPage {
                facts: Vec::new(),
                sections: Vec::new(),
                tables: Vec::new(),
                links: Vec::new(),
                source_html: String::new(),
            },
        }
    }

    fn assumptions() -> EncounterAssumptions {
        EncounterAssumptions {
            assume_player_survives: false,
            player_level: Some(1),
            player_armor: Some(0),
            simultaneous_event_order: SimultaneousEventOrder::PlayerFirst,
            expected_noncritical_player_hit: 10.0,
            player_crit_percent: 0.0,
            player_attack_interval_seconds: 1.0,
            player_max_health: 100.0,
            player_dodge_percent: 20.0,
            expected_post_mitigation_mob_hit: Some(10.0),
            energy: Some(EnergyAssumptions {
                initial_energy: 50.0,
                net_cost_per_second: 5.0,
            }),
        }
    }

    #[test]
    fn opening_hit_and_expected_rates_are_visible() {
        let estimate = estimate_encounter(&mob(Some(true)), assumptions()).unwrap();
        assert_eq!(estimate.expected_player_damage_per_attack, 10.0);
        assert_eq!(estimate.expected_outgoing_dps, 10.0);
        assert_eq!(estimate.expected_time_to_kill_seconds, 9.0);
        assert_eq!(estimate.expected_incoming_dps, 4.0);
        assert_eq!(estimate.expected_health_lost, 36.0);
        assert_eq!(estimate.expected_remaining_health, 64.0);
        assert_eq!(estimate.expected_event_timeline.player_attacks, 10);
        assert_eq!(estimate.expected_event_timeline.mob_attacks, 4);
        assert_eq!(
            estimate.expected_event_timeline.outcome,
            EncounterOutcome::MobDefeated
        );
        assert_eq!(estimate.energy.unwrap().expected_remaining_energy, 5.0);
    }

    #[test]
    fn crit_chain_and_non_attacking_mobs_are_supported() {
        let mut scenario = assumptions();
        scenario.player_crit_percent = 100.0;
        scenario.expected_post_mitigation_mob_hit = None;
        let estimate = estimate_encounter(&mob(Some(false)), scenario).unwrap();
        assert_eq!(estimate.expected_crit_multiplier, 1.99);
        assert_eq!(estimate.expected_incoming_dps, 0.0);
        assert_eq!(estimate.mob_attack_interval_seconds, None);
    }

    #[test]
    fn explicit_survival_policy_skips_incoming_damage() {
        let mut scenario = assumptions();
        scenario.assume_player_survives = true;
        scenario.expected_post_mitigation_mob_hit = None;
        let estimate = estimate_encounter(&mob(Some(true)), scenario).unwrap();
        assert_eq!(estimate.expected_incoming_dps, 0.0);
        assert_eq!(estimate.expected_health_lost, 0.0);
        assert!(estimate.survives_in_expectation);
        assert!(
            estimate
                .notes
                .iter()
                .any(|note| note.contains("assumes the player survives"))
        );
    }

    #[test]
    fn armor_mitigation_uses_expected_raw_mob_damage() {
        let mut target = mob(Some(true));
        target.damage = Some(MobRange { min: 100, max: 200 });

        assert_eq!(expected_mob_hit_after_armor(&target, 10, 0).unwrap(), 150.0);
        assert_eq!(
            expected_mob_hit_after_armor(&target, 10, 700).unwrap(),
            75.0
        );
        assert_eq!(
            expected_mob_hit_after_armor(&target, 10, -350).unwrap(),
            300.0
        );
        assert!(expected_mob_hit_after_armor(&target, 10, -700).is_err());
    }

    #[test]
    fn explicit_mob_hit_override_takes_precedence_over_published_damage() {
        let mut scenario = assumptions();
        scenario.expected_post_mitigation_mob_hit = Some(7.0);
        scenario.player_level = None;
        scenario.player_armor = None;
        let estimate = estimate_encounter(&mob(Some(true)), scenario).unwrap();
        assert_eq!(estimate.expected_mob_damage_per_landed_attack, 7.0);
    }

    #[test]
    fn encounter_derives_mob_hit_from_build_defense() {
        let mut target = mob(Some(true));
        target.damage = Some(MobRange { min: 100, max: 200 });
        let mut scenario = assumptions();
        scenario.expected_post_mitigation_mob_hit = None;
        scenario.apply_build_defense(10, 700);
        let estimate = estimate_encounter(&target, scenario).unwrap();
        assert_eq!(estimate.expected_mob_damage_per_landed_attack, 75.0);
    }

    #[test]
    fn simultaneous_event_order_changes_lethal_ties() {
        let mut scenario = assumptions();
        scenario.expected_noncritical_player_hit = 50.0;
        scenario.player_attack_interval_seconds = 2.0;
        scenario.player_max_health = 10.0;
        scenario.player_dodge_percent = 0.0;
        scenario.expected_post_mitigation_mob_hit = Some(10.0);
        let player_first = estimate_encounter(&mob(Some(true)), scenario.clone()).unwrap();
        assert_eq!(
            player_first.expected_event_timeline.outcome,
            EncounterOutcome::MobDefeated
        );

        scenario.simultaneous_event_order = SimultaneousEventOrder::MobFirst;
        let mob_first = estimate_encounter(&mob(Some(true)), scenario).unwrap();
        assert_eq!(
            mob_first.expected_event_timeline.outcome,
            EncounterOutcome::PlayerDefeated
        );
    }

    #[test]
    fn unknown_mob_behavior_and_invalid_probabilities_are_rejected() {
        assert!(estimate_encounter(&mob(None), assumptions()).is_err());
        let mut invalid = assumptions();
        invalid.player_dodge_percent = 101.0;
        assert!(estimate_encounter(&mob(Some(true)), invalid).is_err());
    }
}
