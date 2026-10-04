//! Character/loadout representation and validation.
//!
//! This module intentionally aggregates sourced values without inventing
//! unresolved combat formulas. Mob-independent basic-attack DPS is derived here;
//! target-specific mitigation and survival belong in encounter models.

use std::collections::{BTreeMap, BTreeSet};

use crate::db::{
    EffectAmount, EffectDirection, EffectResource, ImplantSlot, Item, ItemSet, ItemSetBonusEffect,
    ItemStats, Skill, SkillCooldown, SkillEffectComponent,
};
use crate::mechanics::{
    BASE_ATTACK_POWER, BASE_ATTRIBUTE_VALUE, OUT_OF_COMBAT_REGEN_TICK_SECONDS, Progression,
    UNARMED_ATTACK_INTERVAL_SECONDS, active_set_tier, attack_interval_with_strongest_buff,
    attribute_allocation_budget, basic_attack_power_damage_contribution,
    displayed_crit_from_total_attributes, displayed_dodge_candidate, expected_crit_multiplier,
    out_of_combat_energy_regen_candidate, out_of_combat_health_regen_candidate,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Attributes<T> {
    pub stamina: T,
    pub strength: T,
    pub agility: T,
    pub intellect: T,
}

pub type AttributeAllocation = Attributes<u32>;
pub type AttributeTotals = Attributes<i64>;

impl AttributeAllocation {
    pub fn spent(self) -> Option<u32> {
        self.stamina
            .checked_add(self.strength)?
            .checked_add(self.agility)?
            .checked_add(self.intellect)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum EquipmentSlot {
    Head,
    Face,
    Shoulders,
    Back,
    Chest,
    Legs,
    MainHand,
    OffHand,
    Ring,
    Trinket,
    ImplantBrain,
    ImplantHeart,
    ImplantLeftArm,
    ImplantRightArm,
    ImplantLeftLeg,
    ImplantRightLeg,
    Relic,
    Mount,
    Guild,
    Faction,
    Bag,
    FishingGear,
}

impl EquipmentSlot {
    pub fn accepts(self, item_type: &str) -> bool {
        match self {
            Self::Head => item_type == "Head",
            Self::Face => item_type == "Face",
            Self::Shoulders => item_type == "Shoulders",
            Self::Back => item_type == "Back",
            Self::Chest => item_type == "Chest",
            Self::Legs => item_type == "Legs",
            Self::MainHand => matches!(
                item_type,
                "Axe"
                    | "Bow"
                    | "Mace"
                    | "Mining Tool"
                    | "One-Hand Hammer"
                    | "One-Hand Sword"
                    | "Spear"
                    | "Two-Hand Axe"
                    | "Two-Hand Hammer"
                    | "Two-Hand Staff"
                    | "Two-Hand Sword"
                    | "Unlocking Tool"
                    | "Wand"
                    | "Wood Cutting Tool"
            ),
            Self::OffHand => item_type == "Off-Hand",
            Self::Ring => item_type == "Ring",
            Self::Trinket => item_type == "Trinket",
            Self::ImplantBrain
            | Self::ImplantHeart
            | Self::ImplantLeftArm
            | Self::ImplantRightArm
            | Self::ImplantLeftLeg
            | Self::ImplantRightLeg => item_type == "Implant",
            Self::Relic => item_type == "Relic",
            Self::Mount => item_type == "Mount",
            Self::Guild => item_type == "Guild",
            Self::Faction => item_type == "Faction",
            Self::Bag => item_type == "Bag",
            Self::FishingGear => item_type == "Fishing Gear",
        }
    }

    pub fn accepts_item(self, item: &Item) -> bool {
        if !self.accepts(&item.item_type) {
            return false;
        }
        match self {
            Self::ImplantBrain
            | Self::ImplantHeart
            | Self::ImplantLeftArm
            | Self::ImplantRightArm
            | Self::ImplantLeftLeg
            | Self::ImplantRightLeg => item.implant_slot.map(EquipmentSlot::from) == Some(self),
            _ => true,
        }
    }
}

impl From<ImplantSlot> for EquipmentSlot {
    fn from(slot: ImplantSlot) -> Self {
        match slot {
            ImplantSlot::Brain => Self::ImplantBrain,
            ImplantSlot::Heart => Self::ImplantHeart,
            ImplantSlot::LeftArm => Self::ImplantLeftArm,
            ImplantSlot::RightArm => Self::ImplantRightArm,
            ImplantSlot::LeftLeg => Self::ImplantLeftLeg,
            ImplantSlot::RightLeg => Self::ImplantRightLeg,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct EquippedItem {
    pub slot: EquipmentSlot,
    /// Distinguishes repeated slots such as rings and implants without assuming
    /// a still-unverified slot count. Single-slot equipment normally uses 0.
    #[serde(default)]
    pub slot_index: u8,
    pub item_slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CharacterBuild {
    pub level: u32,
    pub progression: Progression,
    #[serde(default)]
    pub allocated: AttributeAllocation,
    #[serde(default)]
    pub equipment: Vec<EquippedItem>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub active_skill_effects: Vec<ActiveSkillEffect>,
    pub faction_notoriety: Option<u32>,
    pub guild_level: Option<u32>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ActiveSkillRole {
    Buff,
    Pet,
    Morph,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ActiveSkillEffect {
    pub skill_slug: String,
    pub role: ActiveSkillRole,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkillSelectionInspection {
    pub skill_slug: String,
    pub skill_name: String,
    pub rank: Option<u32>,
    pub active_roles: Vec<ActiveSkillRole>,
    pub effect_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ActiveEnergyRegenEffect {
    pub skill_slug: String,
    pub skill_name: String,
    pub rank: Option<u32>,
    pub amount: EffectAmount,
    pub interval_seconds: u32,
    pub expected_energy_per_second: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ActiveEnergyRegenSummary {
    pub expected_energy_per_second: f64,
    pub effects: Vec<ActiveEnergyRegenEffect>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AttackSkillRecommendation {
    pub skill_slug: String,
    pub skill_name: String,
    pub rank: Option<u32>,
    pub damage_min: u32,
    pub damage_max: u32,
    pub expected_noncritical_damage: f64,
    pub expected_crit_multiplier: f64,
    pub expected_damage_per_cast: f64,
    pub cast_time_seconds: f64,
    pub cooldown_seconds: Option<f64>,
    pub effective_repeat_seconds: f64,
    pub standalone_expected_dps: f64,
    pub energy_cost: u32,
    pub timing_limited_casts_per_second: f64,
    pub energy_regen_per_second: f64,
    pub energy_limited_casts_per_second: f64,
    pub sustained_casts_per_second: f64,
    pub sustained_expected_dps: f64,
    pub energy_per_second_at_timing_limit: f64,
    pub is_energy_sustainable: bool,
    pub full_energy_casts: u32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AttackSkillRecommendationSummary {
    pub top: Vec<AttackSkillRecommendation>,
    pub direct_damage_records_evaluated: usize,
    pub castable_distinct_skills: usize,
    pub excluded_by_requirements_or_energy: usize,
    pub model_notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct AggregatedItemStats {
    pub armor: i64,
    pub stamina: i64,
    pub strength: i64,
    pub agility: i64,
    pub intellect: i64,
    pub attack_power: i64,
    pub crit: i64,
    pub flat_damage: i64,
    pub cast_time_reduction: i64,
    pub max_health: i64,
    pub max_energy: i64,
    pub health_regen: i64,
    pub energy_regen: i64,
    /// Strongest active reduction in milliseconds, represented as a positive value.
    pub attack_speed_reduction_ms: i64,
}

impl AggregatedItemStats {
    fn from_item_stats(stats: &ItemStats) -> Self {
        let mut totals = Self::default();
        totals.add(stats);
        totals
    }

    fn add(&mut self, stats: &ItemStats) {
        self.armor += i64::from(stats.armor.unwrap_or(0));
        self.stamina += i64::from(stats.stamina.unwrap_or(0));
        self.strength += i64::from(stats.strength.unwrap_or(0));
        self.agility += i64::from(stats.agility.unwrap_or(0));
        self.intellect += i64::from(stats.intellect.unwrap_or(0));
        self.attack_power += i64::from(stats.attack_power.unwrap_or(0));
        self.crit += i64::from(stats.crit.unwrap_or(0));
        self.flat_damage += i64::from(stats.damage.unwrap_or(0));
        self.cast_time_reduction += i64::from(stats.cast_time_reduction.unwrap_or(0));
        self.max_health += i64::from(stats.max_health.unwrap_or(0));
        self.max_energy += i64::from(stats.max_energy.unwrap_or(0));
        self.health_regen += i64::from(stats.health_regen.unwrap_or(0));
        self.energy_regen += i64::from(stats.energy_regen.unwrap_or(0));
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EquippedItemInspection {
    pub slot: EquipmentSlot,
    pub slot_index: u8,
    pub item_slug: String,
    pub name: String,
    pub item_type: String,
    pub damage_min: Option<u32>,
    pub damage_max: Option<u32>,
    pub attack_speed: Option<f32>,
    pub stats: AggregatedItemStats,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ConfirmedDerivedStats {
    pub maximum_health: i64,
    pub maximum_energy: i64,
    pub armor: i64,
    pub attack_power: Option<i64>,
    pub panel_damage_min: Option<u32>,
    pub panel_damage_max: Option<u32>,
    pub expected_noncritical_basic_attack_hit: Option<f64>,
    pub expected_basic_attack_dps: Option<f64>,
    pub attack_interval_seconds: f64,
    pub displayed_crit_percent: Option<f64>,
    pub displayed_dodge_percent: Option<f64>,
    pub out_of_combat_health_regen_per_tick_candidate: f64,
    pub out_of_combat_energy_regen_per_tick_candidate: f64,
    pub out_of_combat_regen_tick_seconds: u32,
    pub out_of_combat_regen_note: String,
    pub periodic_active_regen_note: String,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ActiveSetTier {
    pub set_name: String,
    pub set_slug: String,
    pub equipped_pieces: u32,
    pub required_pieces: u32,
    pub effects: Vec<ItemSetBonusEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildIssueKind {
    Error,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BuildIssue {
    pub kind: BuildIssueKind,
    pub source_slug: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BuildInspection {
    pub level: u32,
    pub progression: Progression,
    pub allocation_budget: u32,
    pub allocated_points: u32,
    pub unspent_points: u32,
    pub item_stats: AggregatedItemStats,
    pub set_bonus_stats: AggregatedItemStats,
    pub active_effect_stats: AggregatedItemStats,
    /// Base + allocated + item attributes. Item attribute requirements are
    /// informational because the game permits overequipping; locally cast
    /// pets and attack skills still validate against this total.
    /// Whether set bonuses also satisfy skill requirements is not yet confirmed.
    pub requirement_attributes: AttributeTotals,
    /// Requirement attributes plus the active highest set-tier attributes.
    pub final_attributes: AttributeTotals,
    pub equipment: Vec<EquippedItemInspection>,
    pub active_set_tiers: Vec<ActiveSetTier>,
    pub skills: Vec<SkillSelectionInspection>,
    pub active_energy_regen: ActiveEnergyRegenSummary,
    pub recommended_attack_skills: AttackSkillRecommendationSummary,
    pub confirmed_derived_stats: ConfirmedDerivedStats,
    pub issues: Vec<BuildIssue>,
}

impl BuildInspection {
    pub fn is_valid(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.kind == BuildIssueKind::Error)
    }
}

pub fn inspect_build(
    build: &CharacterBuild,
    items: &[Item],
    item_sets: &[ItemSet],
    skills: &[Skill],
) -> BuildInspection {
    let item_by_slug: BTreeMap<_, _> = items
        .iter()
        .map(|item| (item.slug.as_str(), item))
        .collect();
    let budget = attribute_allocation_budget(build.level, build.progression).unwrap_or(u32::MAX);
    let spent = build.allocated.spent().unwrap_or(u32::MAX);
    let mut issues = Vec::new();
    if spent > budget {
        issues.push(BuildIssue {
            kind: BuildIssueKind::Error,
            source_slug: None,
            message: format!("allocated {spent} points but the progression budget is {budget}"),
        });
    }

    let mut occupied = BTreeSet::new();
    let mut equipped_items = Vec::new();
    let mut equipment_inspection = Vec::new();
    let mut item_stats = AggregatedItemStats::default();
    for equipped in &build.equipment {
        if !occupied.insert((equipped.slot, equipped.slot_index)) {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Error,
                source_slug: Some(equipped.item_slug.clone()),
                message: format!(
                    "equipment slot {:?}[{}] is occupied more than once",
                    equipped.slot, equipped.slot_index
                ),
            });
        }
        let Some(item) = item_by_slug.get(equipped.item_slug.as_str()).copied() else {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Error,
                source_slug: Some(equipped.item_slug.clone()),
                message: "item slug is absent from the item snapshot".into(),
            });
            continue;
        };
        if !equipped.slot.accepts_item(item) {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Error,
                source_slug: Some(item.slug.clone()),
                message: format!(
                    "item type {:?} is incompatible with slot {:?}",
                    item.item_type, equipped.slot
                ),
            });
        }
        item_stats.add(&item.stats);
        equipment_inspection.push(EquippedItemInspection {
            slot: equipped.slot,
            slot_index: equipped.slot_index,
            item_slug: item.slug.clone(),
            name: item.name.clone(),
            item_type: item.item_type.clone(),
            damage_min: item.damage_min,
            damage_max: item.damage_max,
            attack_speed: item.attack_speed,
            stats: AggregatedItemStats::from_item_stats(&item.stats),
        });
        equipped_items.push(item);
    }

    let base = i64::from(BASE_ATTRIBUTE_VALUE);
    let requirement_attributes = AttributeTotals {
        stamina: base + i64::from(build.allocated.stamina) + item_stats.stamina,
        strength: base + i64::from(build.allocated.strength) + item_stats.strength,
        agility: base + i64::from(build.allocated.agility) + item_stats.agility,
        intellect: base + i64::from(build.allocated.intellect) + item_stats.intellect,
    };

    for item in &equipped_items {
        validate_item_requirements(build, item, &mut issues);
    }

    let gathering_tool_in_main_hand = build.equipment.iter().any(|equipped| {
        equipped.slot == EquipmentSlot::MainHand
            && item_by_slug
                .get(equipped.item_slug.as_str())
                .is_some_and(|item| {
                    matches!(
                        item.item_type.as_str(),
                        "Mining Tool" | "Unlocking Tool" | "Wood Cutting Tool"
                    )
                })
    });
    let equipped_counts: BTreeMap<&str, u32> =
        equipped_items
            .iter()
            .fold(BTreeMap::new(), |mut counts, item| {
                *counts.entry(item.slug.as_str()).or_default() += 1;
                counts
            });
    let mut active_set_tiers = Vec::new();
    for item_set in item_sets {
        let equipped_pieces = item_set
            .pieces
            .iter()
            .map(|piece| {
                equipped_counts
                    .get(piece.slug.as_str())
                    .copied()
                    .unwrap_or(0)
            })
            .sum();
        if let Some(tier) = active_set_tier(
            &item_set.bonuses,
            equipped_pieces,
            gathering_tool_in_main_hand,
        ) {
            active_set_tiers.push(ActiveSetTier {
                set_name: item_set.name.clone(),
                set_slug: item_set.slug.clone(),
                equipped_pieces,
                required_pieces: tier.required_pieces,
                effects: tier.effects.clone(),
            });
        }
    }

    let mut set_bonus_stats = AggregatedItemStats::default();
    for tier in &active_set_tiers {
        for effect in &tier.effects {
            if let Err(message) = add_set_effect(&mut set_bonus_stats, effect) {
                issues.push(BuildIssue {
                    kind: BuildIssueKind::Unresolved,
                    source_slug: Some(tier.set_slug.clone()),
                    message,
                });
            }
        }
    }
    let selected_skills = inspect_skills(build, skills, requirement_attributes, &mut issues);
    let active_effect_stats = aggregate_active_effect_stats(build, skills);
    let active_energy_regen = aggregate_active_energy_regen(build, skills);
    let final_attributes = AttributeTotals {
        stamina: requirement_attributes.stamina
            + set_bonus_stats.stamina
            + active_effect_stats.stamina,
        strength: requirement_attributes.strength
            + set_bonus_stats.strength
            + active_effect_stats.strength,
        agility: requirement_attributes.agility
            + set_bonus_stats.agility
            + active_effect_stats.agility,
        intellect: requirement_attributes.intellect
            + set_bonus_stats.intellect
            + active_effect_stats.intellect,
    };
    let confirmed_derived_stats = confirmed_derived_stats(
        build,
        &equipment_inspection,
        item_stats,
        set_bonus_stats,
        active_effect_stats,
        final_attributes,
        &mut issues,
    );
    let recommended_attack_skills = recommend_attack_skills(
        build,
        skills,
        requirement_attributes,
        &confirmed_derived_stats,
        active_energy_regen.expected_energy_per_second,
    );

    BuildInspection {
        level: build.level,
        progression: build.progression,
        allocation_budget: budget,
        allocated_points: spent,
        unspent_points: budget.saturating_sub(spent),
        item_stats,
        set_bonus_stats,
        active_effect_stats,
        requirement_attributes,
        final_attributes,
        equipment: equipment_inspection,
        active_set_tiers,
        skills: selected_skills,
        active_energy_regen,
        recommended_attack_skills,
        confirmed_derived_stats,
        issues,
    }
}

fn confirmed_derived_stats(
    build: &CharacterBuild,
    equipment: &[EquippedItemInspection],
    item_stats: AggregatedItemStats,
    set_stats: AggregatedItemStats,
    active_stats: AggregatedItemStats,
    final_attributes: AttributeTotals,
    issues: &mut Vec<BuildIssue>,
) -> ConfirmedDerivedStats {
    let base = i64::from(BASE_ATTRIBUTE_VALUE);
    let attribute_points = [
        final_attributes.stamina - base,
        final_attributes.strength - base,
        final_attributes.agility - base,
        final_attributes.intellect - base,
    ];
    let maximum_health = i64::from(build.level) * 18
        + attribute_points[0] * 20
        + item_stats.max_health
        + set_stats.max_health
        + active_stats.max_health;
    let maximum_energy = i64::from(build.level) * 20
        + attribute_points[3] * 15
        + item_stats.max_energy
        + set_stats.max_energy
        + active_stats.max_energy;
    let armor = attribute_points[1] * 5 + item_stats.armor + set_stats.armor + active_stats.armor;
    let maximum_attribute_points = attribute_points.iter().copied().max().unwrap_or(0);
    let highest_count = attribute_points
        .iter()
        .filter(|value| **value == maximum_attribute_points)
        .count();
    let attribute_attack_power = if maximum_attribute_points == 0
        && attribute_points.iter().all(|points| *points == 0)
    {
        Some(0i64)
    } else if highest_count == 1 {
        let highest_index = attribute_points
            .iter()
            .position(|value| *value == maximum_attribute_points)
            .expect("one final attribute is uniquely highest");
        let strength_base = attribute_points[1];
        let highest_bonus = attribute_points[highest_index] * 2;
        Some(strength_base + highest_bonus)
    } else {
        issues.push(BuildIssue {
            kind: BuildIssueKind::Unresolved,
            source_slug: None,
            message: "Attack Power is unresolved because multiple final attributes tie for highest"
                .into(),
        });
        None
    };
    let attack_power = attribute_attack_power.map(|attribute_contribution| {
        i64::from(BASE_ATTACK_POWER)
            + attribute_contribution
            + item_stats.attack_power
            + set_stats.attack_power
            + active_stats.attack_power
    });
    let main_hand = equipment
        .iter()
        .find(|item| item.slot == EquipmentSlot::MainHand);
    let base_attack_interval_seconds = main_hand
        .and_then(|item| item.attack_speed)
        .map(f64::from)
        .unwrap_or(UNARMED_ATTACK_INTERVAL_SECONDS);
    let attack_interval_seconds = if active_stats.attack_speed_reduction_ms > 0 {
        attack_interval_with_strongest_buff(
            base_attack_interval_seconds,
            &[u32::try_from(active_stats.attack_speed_reduction_ms).unwrap_or(u32::MAX)],
        )
        .unwrap_or_else(|| {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Unresolved,
                source_slug: None,
                message: "active attack-speed reduction reaches the still-unconfirmed minimum attack interval".into(),
            });
            base_attack_interval_seconds
        })
    } else {
        base_attack_interval_seconds
    };
    let implant_damage = equipment
        .iter()
        .filter(|item| item.item_type == "Implant")
        .map(|item| item.stats.flat_damage)
        .sum::<i64>();
    let panel_damage = attack_power.and_then(|attack_power| {
        let contribution =
            basic_attack_power_damage_contribution(attack_power, base_attack_interval_seconds)?;
        let (base_min, base_max) = if let Some(weapon) = main_hand {
            (i64::from(weapon.damage_min?), i64::from(weapon.damage_max?))
        } else {
            (
                i64::from(crate::mechanics::UNARMED_INTRINSIC_DAMAGE_MIN),
                i64::from(crate::mechanics::UNARMED_INTRINSIC_DAMAGE_MAX),
            )
        };
        Some((
            u32::try_from(base_min + contribution + implant_damage).ok()?,
            u32::try_from(base_max + contribution + implant_damage).ok()?,
        ))
    });
    let total_agility = u32::try_from(final_attributes.agility).ok();
    let total_intellect = u32::try_from(final_attributes.intellect).ok();
    let direct_crit = u32::try_from(item_stats.crit + set_stats.crit + active_stats.crit).ok();
    let displayed_crit_percent =
        total_agility
            .zip(total_intellect)
            .zip(direct_crit)
            .map(|((agility, intellect), crit)| {
                displayed_crit_from_total_attributes(agility, intellect, crit)
            });
    let displayed_dodge_percent = total_agility.map(displayed_dodge_candidate);
    let expected_noncritical_basic_attack_hit =
        panel_damage.map(|(minimum, maximum)| (f64::from(minimum) + f64::from(maximum)) / 2.0);
    let expected_basic_attack_dps = expected_noncritical_basic_attack_hit
        .zip(displayed_crit_percent)
        .and_then(|(noncritical_hit, crit_percent)| {
            expected_crit_multiplier(crit_percent)
                .map(|crit_multiplier| noncritical_hit * crit_multiplier / attack_interval_seconds)
        });
    let mut unresolved = Vec::new();

    unresolved.push(
        "Out-of-combat base regeneration formulas fit all current observations but remain bounded candidates; the Intellect Energy Regen contribution still needs a controlled allocation test".into(),
    );
    unresolved.push(
        "Equipment regeneration units and interaction with out-of-combat regeneration remain unknown".into(),
    );

    ConfirmedDerivedStats {
        maximum_health,
        maximum_energy,
        armor,
        attack_power,
        panel_damage_min: panel_damage.map(|damage| damage.0),
        panel_damage_max: panel_damage.map(|damage| damage.1),
        expected_noncritical_basic_attack_hit,
        expected_basic_attack_dps,
        attack_interval_seconds,
        displayed_crit_percent,
        displayed_dodge_percent,
        out_of_combat_health_regen_per_tick_candidate: out_of_combat_health_regen_candidate(
            build.level,
            build.allocated.stamina,
            build.allocated.strength,
        ),
        out_of_combat_energy_regen_per_tick_candidate: out_of_combat_energy_regen_candidate(
            build.level,
            build.allocated.intellect,
        ),
        out_of_combat_regen_tick_seconds: OUT_OF_COMBAT_REGEN_TICK_SECONDS,
        out_of_combat_regen_note: "The two-second timer continues during combat, but ready ticks are suppressed; if it reaches zero during combat, regeneration procs immediately when combat ends".into(),
        periodic_active_regen_note: "Skill, buff, and pet regeneration uses each effect's published amount and interval and remains active while that effect is active".into(),
        unresolved,
    }
}

fn recommend_attack_skills(
    build: &CharacterBuild,
    skills: &[Skill],
    requirement_attributes: AttributeTotals,
    derived: &ConfirmedDerivedStats,
    active_energy_regen_per_second: f64,
) -> AttackSkillRecommendationSummary {
    let crit_percent = derived.displayed_crit_percent;
    let crit_multiplier = crit_percent.and_then(expected_crit_multiplier);
    let mut evaluated = 0;
    let mut excluded = 0;
    let mut best_by_name = BTreeMap::<String, AttackSkillRecommendation>::new();

    for skill in skills {
        let damage_amounts: Vec<_> = skill
            .effects
            .iter()
            .flat_map(|effect| &effect.components)
            .filter_map(|component| match component {
                SkillEffectComponent::DirectDamage { amount } => Some(amount),
                _ => None,
            })
            .collect();
        if damage_amounts.is_empty() {
            continue;
        }
        evaluated += 1;

        let requirements_met = !skill
            .level_requirement
            .is_some_and(|required| build.level < required)
            && skill.attribute_requirements.iter().all(|requirement| {
                match (
                    requirement.attribute.to_ascii_lowercase().as_str(),
                    requirement.amount,
                ) {
                    ("stamina", Some(required)) => {
                        requirement_attributes.stamina >= i64::from(required)
                    }
                    ("strength", Some(required)) => {
                        requirement_attributes.strength >= i64::from(required)
                    }
                    ("agility", Some(required)) => {
                        requirement_attributes.agility >= i64::from(required)
                    }
                    ("intellect", Some(required)) => {
                        requirement_attributes.intellect >= i64::from(required)
                    }
                    ("progression", None) if requirement.value.eq_ignore_ascii_case("rebirth") => {
                        matches!(
                            build.progression,
                            Progression::Rebirth | Progression::Ascension
                        )
                    }
                    ("progression", None)
                        if requirement.value.eq_ignore_ascii_case("ascension") =>
                    {
                        build.progression == Progression::Ascension
                    }
                    _ => false,
                }
            });
        let Some(energy_cost) = skill.energy_cost else {
            excluded += 1;
            continue;
        };
        if !requirements_met || i64::from(energy_cost) > derived.maximum_energy {
            excluded += 1;
            continue;
        }
        let (Some(cast_time), Some(cooldown), Some(crit_multiplier)) =
            (skill.cast_time, skill.cooldown.as_ref(), crit_multiplier)
        else {
            excluded += 1;
            continue;
        };
        let cast_time_seconds = f64::from(cast_time);
        let cooldown_seconds = match cooldown {
            SkillCooldown::None => None,
            SkillCooldown::Seconds(seconds) => Some(f64::from(*seconds)),
        };
        let effective_repeat_seconds = cast_time_seconds
            .max(cooldown_seconds.unwrap_or(0.0))
            .max(1.0);
        if !effective_repeat_seconds.is_finite() || effective_repeat_seconds <= 0.0 {
            excluded += 1;
            continue;
        }
        let damage = damage_amounts
            .iter()
            .try_fold((0_u32, 0_u32), |totals, amount| {
                let (minimum, maximum) = match amount {
                    EffectAmount::Fixed(value) => (*value, *value),
                    EffectAmount::Range { min, max } => (*min, *max),
                };
                Some((
                    totals.0.checked_add(minimum)?,
                    totals.1.checked_add(maximum)?,
                ))
            });
        let Some((damage_min, damage_max)) = damage else {
            excluded += 1;
            continue;
        };
        let expected_noncritical_damage = (f64::from(damage_min) + f64::from(damage_max)) / 2.0;
        let expected_damage_per_cast = expected_noncritical_damage * crit_multiplier;
        let timing_limited_casts_per_second = 1.0 / effective_repeat_seconds;
        let energy_limited_casts_per_second =
            active_energy_regen_per_second / f64::from(energy_cost);
        let sustained_casts_per_second =
            timing_limited_casts_per_second.min(energy_limited_casts_per_second);
        let energy_per_second_at_timing_limit =
            timing_limited_casts_per_second * f64::from(energy_cost);
        let recommendation = AttackSkillRecommendation {
            skill_slug: skill.slug.clone(),
            skill_name: skill.name.clone(),
            rank: skill.rank,
            damage_min,
            damage_max,
            expected_noncritical_damage,
            expected_crit_multiplier: crit_multiplier,
            expected_damage_per_cast,
            cast_time_seconds,
            cooldown_seconds,
            effective_repeat_seconds,
            standalone_expected_dps: expected_damage_per_cast / effective_repeat_seconds,
            energy_cost,
            timing_limited_casts_per_second,
            energy_regen_per_second: active_energy_regen_per_second,
            energy_limited_casts_per_second,
            sustained_casts_per_second,
            sustained_expected_dps: expected_damage_per_cast * sustained_casts_per_second,
            energy_per_second_at_timing_limit,
            is_energy_sustainable: active_energy_regen_per_second
                >= energy_per_second_at_timing_limit,
            full_energy_casts: if energy_cost == 0 {
                u32::MAX
            } else {
                u32::try_from(derived.maximum_energy / i64::from(energy_cost)).unwrap_or(u32::MAX)
            },
        };
        let replace = best_by_name.get(&skill.name).is_none_or(|current| {
            recommendation.standalone_expected_dps > current.standalone_expected_dps
        });
        if replace {
            best_by_name.insert(skill.name.clone(), recommendation);
        }
    }

    let castable_distinct_skills = best_by_name.len();
    let mut top: Vec<_> = best_by_name.into_values().collect();
    top.sort_by(|left, right| {
        right
            .sustained_expected_dps
            .total_cmp(&left.sustained_expected_dps)
            .then_with(|| {
                right
                    .standalone_expected_dps
                    .total_cmp(&left.standalone_expected_dps)
            })
            .then_with(|| left.skill_name.cmp(&right.skill_name))
    });
    top.truncate(3);

    AttackSkillRecommendationSummary {
        top,
        direct_damage_records_evaluated: evaluated,
        castable_distinct_skills,
        excluded_by_requirements_or_energy: excluded,
        model_notes: vec![
            "Only skills whose local level, progression, attribute, and one-cast Energy requirements are met are eligible.".into(),
            "Expected damage uses the published damage-range midpoint and the build's confirmed chained-Crit expectation; attributes and Attack Power do not scale active skill damage.".into(),
            "Burst DPS divides expected damage per cast by max(base cast time, cooldown, 1-second cast-request interval).".into(),
            "Sustained DPS uses the lower of the timing-limited cast rate and active in-combat Energy regeneration divided by Energy cost. Published periodic ticks are represented by their long-run expected Energy per second; initial Energy is reported as full-energy casts but is not amortized over an invented fight duration.".into(),
            "Recommendations rank by sustained DPS, then burst DPS. This remains a single-skill comparison, not a multi-skill rotation.".into(),
            "Only the strongest castable rank of each skill name can occupy the top three.".into(),
        ],
    }
}

fn add_set_effect(
    totals: &mut AggregatedItemStats,
    effect: &ItemSetBonusEffect,
) -> Result<(), String> {
    let value = effect
        .value
        .replace(',', "")
        .parse::<i64>()
        .map_err(|_| format!("unparsed set effect: {} {}", effect.stat, effect.value))?;
    match effect.stat.as_str() {
        "Armor" => totals.armor += value,
        "Stamina" => totals.stamina += value,
        "Strength" => totals.strength += value,
        "Agility" => totals.agility += value,
        "Intellect" => totals.intellect += value,
        "Attack power" => totals.attack_power += value,
        "Crit" => totals.crit += value,
        _ => {
            return Err(format!(
                "unsupported set effect: {} {}",
                effect.stat, effect.value
            ));
        }
    }
    Ok(())
}

fn aggregate_active_effect_stats(build: &CharacterBuild, skills: &[Skill]) -> AggregatedItemStats {
    let skill_by_slug: BTreeMap<_, _> = skills
        .iter()
        .map(|skill| (skill.slug.as_str(), skill))
        .collect();
    let selected: BTreeSet<_> = build.skills.iter().map(String::as_str).collect();
    let mut active_keys = BTreeSet::new();
    let mut totals = AggregatedItemStats::default();

    for active in &build.active_skill_effects {
        if !active_keys.insert((active.skill_slug.as_str(), active.role))
            || !selected.contains(active.skill_slug.as_str())
        {
            continue;
        }
        let Some(skill) = skill_by_slug.get(active.skill_slug.as_str()).copied() else {
            continue;
        };
        if !skill_supports_role(skill, active.role) {
            continue;
        }
        for component in skill.effects.iter().flat_map(|effect| &effect.components) {
            let SkillEffectComponent::TimedStatModifier { stat, value } = component else {
                continue;
            };
            let value = i64::from(*value);
            match stat.as_str() {
                "Armor" => totals.armor += value,
                "Stamina" => totals.stamina += value,
                "Strength" => totals.strength += value,
                "Agility" => totals.agility += value,
                "Intellect" => totals.intellect += value,
                "Attack power" => totals.attack_power += value,
                "Crit" => totals.crit += value,
                "Max health" => totals.max_health += value,
                "Max energy" => totals.max_energy += value,
                "Attack speed" if value < 0 => {
                    totals.attack_speed_reduction_ms = totals.attack_speed_reduction_ms.max(-value);
                }
                _ => {}
            }
        }
    }
    totals
}

fn aggregate_active_energy_regen(
    build: &CharacterBuild,
    skills: &[Skill],
) -> ActiveEnergyRegenSummary {
    let skill_by_slug: BTreeMap<_, _> = skills
        .iter()
        .map(|skill| (skill.slug.as_str(), skill))
        .collect();
    let selected: BTreeSet<_> = build.skills.iter().map(String::as_str).collect();
    let mut active_keys = BTreeSet::new();
    let mut effects = Vec::new();

    for active in &build.active_skill_effects {
        if !active_keys.insert((active.skill_slug.as_str(), active.role))
            || !selected.contains(active.skill_slug.as_str())
        {
            continue;
        }
        let Some(skill) = skill_by_slug.get(active.skill_slug.as_str()).copied() else {
            continue;
        };
        if !skill_supports_role(skill, active.role) {
            continue;
        }
        for component in skill.effects.iter().flat_map(|effect| &effect.components) {
            let SkillEffectComponent::Periodic {
                resource: EffectResource::Energy,
                direction: EffectDirection::Gain,
                amount,
                interval_seconds,
            } = component
            else {
                continue;
            };
            if *interval_seconds == 0 {
                continue;
            }
            let expected_amount = match amount {
                EffectAmount::Fixed(value) => f64::from(*value),
                EffectAmount::Range { min, max } => (f64::from(*min) + f64::from(*max)) / 2.0,
            };
            effects.push(ActiveEnergyRegenEffect {
                skill_slug: skill.slug.clone(),
                skill_name: skill.name.clone(),
                rank: skill.rank,
                amount: amount.clone(),
                interval_seconds: *interval_seconds,
                expected_energy_per_second: expected_amount / f64::from(*interval_seconds),
            });
        }
    }
    ActiveEnergyRegenSummary {
        expected_energy_per_second: effects
            .iter()
            .map(|effect| effect.expected_energy_per_second)
            .sum(),
        effects,
    }
}

fn inspect_skills(
    build: &CharacterBuild,
    skills: &[Skill],
    attributes: AttributeTotals,
    issues: &mut Vec<BuildIssue>,
) -> Vec<SkillSelectionInspection> {
    let skill_by_slug: BTreeMap<_, _> = skills
        .iter()
        .map(|skill| (skill.slug.as_str(), skill))
        .collect();
    let active_buff_count = build
        .active_skill_effects
        .iter()
        .filter(|active| matches!(active.role, ActiveSkillRole::Buff | ActiveSkillRole::Morph))
        .count();
    if active_buff_count > 5 {
        issues.push(BuildIssue {
            kind: BuildIssueKind::Error,
            source_slug: None,
            message: format!("at most 5 buffs/morphs may be active; selected {active_buff_count}"),
        });
    }
    let active_pet_count = build
        .active_skill_effects
        .iter()
        .filter(|active| active.role == ActiveSkillRole::Pet)
        .count();
    if active_pet_count > 1 {
        issues.push(BuildIssue {
            kind: BuildIssueKind::Error,
            source_slug: None,
            message: format!("at most 1 pet may be active; selected {active_pet_count}"),
        });
    }
    if active_pet_count > 0 && build.progression == Progression::Spawn {
        issues.push(BuildIssue {
            kind: BuildIssueKind::Error,
            source_slug: None,
            message: "pets require Rebirth or Ascension progression".into(),
        });
    }

    let mut learned = BTreeSet::new();
    let mut selected = Vec::new();
    let externally_supplied: BTreeSet<&str> = build
        .active_skill_effects
        .iter()
        .filter(|active| matches!(active.role, ActiveSkillRole::Buff | ActiveSkillRole::Morph))
        .filter_map(|active| {
            let skill = skill_by_slug.get(active.skill_slug.as_str()).copied()?;
            skill_supports_role(skill, active.role).then_some(active.skill_slug.as_str())
        })
        .collect();

    for slug in &build.skills {
        if !learned.insert(slug.as_str()) {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Error,
                source_slug: Some(slug.clone()),
                message: "skill is selected more than once".into(),
            });
            continue;
        }
        let Some(skill) = skill_by_slug.get(slug.as_str()).copied() else {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Error,
                source_slug: Some(slug.clone()),
                message: "skill slug is absent from the skill snapshot".into(),
            });
            continue;
        };
        validate_skill_requirements(
            build,
            skill,
            attributes,
            externally_supplied.contains(slug.as_str()),
            issues,
        );
        selected.push(SkillSelectionInspection {
            skill_slug: slug.clone(),
            skill_name: skill.name.clone(),
            rank: skill.rank,
            active_roles: Vec::new(),
            effect_types: skill
                .effects
                .iter()
                .map(|effect| effect.effect_type.clone())
                .collect(),
        });
    }

    let mut active_keys = BTreeSet::new();
    for active in &build.active_skill_effects {
        if !active_keys.insert((active.skill_slug.as_str(), active.role)) {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Error,
                source_slug: Some(active.skill_slug.clone()),
                message: format!("active role {:?} is selected more than once", active.role),
            });
            continue;
        }
        if !learned.contains(active.skill_slug.as_str()) {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Error,
                source_slug: Some(active.skill_slug.clone()),
                message: "active skill effect must reference a selected skill".into(),
            });
            continue;
        }
        let Some(skill) = skill_by_slug.get(active.skill_slug.as_str()).copied() else {
            continue;
        };
        if !skill_supports_role(skill, active.role) {
            issues.push(BuildIssue {
                kind: BuildIssueKind::Error,
                source_slug: Some(active.skill_slug.clone()),
                message: format!(
                    "skill has no typed component supporting role {:?}",
                    active.role
                ),
            });
            continue;
        }
        if let Some(summary) = selected
            .iter_mut()
            .find(|summary| summary.skill_slug == active.skill_slug)
        {
            summary.active_roles.push(active.role);
        }
    }
    selected
}

pub(crate) fn skill_supports_role(skill: &Skill, role: ActiveSkillRole) -> bool {
    let components: Vec<_> = skill
        .effects
        .iter()
        .flat_map(|effect| &effect.components)
        .collect();
    match role {
        ActiveSkillRole::Buff => {
            components.iter().any(|component| {
                matches!(
                    component,
                    SkillEffectComponent::TimedStatModifier { .. }
                        | SkillEffectComponent::Periodic { .. }
                        | SkillEffectComponent::Morph
                )
            }) && !components
                .iter()
                .any(|component| matches!(component, SkillEffectComponent::Pet))
        }
        ActiveSkillRole::Pet => components
            .iter()
            .any(|component| matches!(component, SkillEffectComponent::Pet)),
        ActiveSkillRole::Morph => components
            .iter()
            .any(|component| matches!(component, SkillEffectComponent::Morph)),
    }
}

fn validate_skill_requirements(
    build: &CharacterBuild,
    skill: &Skill,
    attributes: AttributeTotals,
    externally_supplied: bool,
    issues: &mut Vec<BuildIssue>,
) {
    if skill
        .level_requirement
        .is_some_and(|required| build.level < required)
    {
        push_skill_issue(
            issues,
            BuildIssueKind::Error,
            skill,
            format!(
                "requires level {}, character is level {}",
                skill.level_requirement.unwrap(),
                build.level
            ),
        );
    }
    if externally_supplied {
        return;
    }
    for requirement in &skill.attribute_requirements {
        let actual = match requirement.attribute.to_ascii_lowercase().as_str() {
            "stamina" => Some(attributes.stamina),
            "strength" => Some(attributes.strength),
            "agility" => Some(attributes.agility),
            "intellect" => Some(attributes.intellect),
            _ => None,
        };
        match (actual, requirement.amount) {
            (Some(actual), Some(required)) if actual < i64::from(required) => push_skill_issue(
                issues,
                BuildIssueKind::Error,
                skill,
                format!(
                    "requires {} {required}, final equipped total is {actual}",
                    requirement.attribute
                ),
            ),
            (None, _) | (_, None) => push_skill_issue(
                issues,
                BuildIssueKind::Unresolved,
                skill,
                format!("unresolved skill requirement: {}", requirement.value),
            ),
            _ => {}
        }
    }
}

fn push_skill_issue(
    issues: &mut Vec<BuildIssue>,
    kind: BuildIssueKind,
    skill: &Skill,
    message: String,
) {
    issues.push(BuildIssue {
        kind,
        source_slug: Some(skill.slug.clone()),
        message,
    });
}

fn validate_item_requirements(build: &CharacterBuild, item: &Item, issues: &mut Vec<BuildIssue>) {
    if item
        .requirements
        .level
        .is_some_and(|required| build.level < required)
    {
        push_item_error(
            issues,
            item,
            format!(
                "requires level {}, character is level {}",
                item.requirements.level.unwrap(),
                build.level
            ),
        );
    }

    if item.rebirth && build.progression == Progression::Spawn {
        push_item_error(
            issues,
            item,
            "requires Rebirth or Ascension progression".into(),
        );
    }
    if item.ascension && build.progression != Progression::Ascension {
        push_item_error(issues, item, "requires Ascension progression".into());
    }
    for (name, actual, required) in [
        (
            "faction notoriety",
            build.faction_notoriety,
            item.requirements.faction_notoriety,
        ),
        (
            "guild level",
            build.guild_level,
            item.requirements.guild_level,
        ),
    ] {
        match (actual, required) {
            (Some(actual), Some(required)) if actual < required => push_item_error(
                issues,
                item,
                format!("requires {name} {required}, character has {actual}"),
            ),
            (None, Some(required)) => issues.push(BuildIssue {
                kind: BuildIssueKind::Unresolved,
                source_slug: Some(item.slug.clone()),
                message: format!(
                    "requires {name} {required}, but the build does not specify {name}"
                ),
            }),
            _ => {}
        }
    }
}

fn push_item_error(issues: &mut Vec<BuildIssue>, item: &Item, message: String) {
    issues.push(BuildIssue {
        kind: BuildIssueKind::Error,
        source_slug: Some(item.slug.clone()),
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        EffectAmount, ItemRequirements, ItemSetBonus, ItemSetPiece, SkillAttributeRequirement,
        SkillEffect,
    };

    fn item(slug: &str, item_type: &str, stats: ItemStats, requirements: ItemRequirements) -> Item {
        Item {
            name: slug.into(),
            slug: slug.into(),
            item_type: item_type.into(),
            description: None,
            image_url: None,
            implant_slot: None,
            damage_min: None,
            damage_max: None,
            attack_speed: None,
            stats,
            requirements,
            market_price: None,
            recently_sold_price: None,
            shop_price: None,
            sellable_to_shops: false,
            rebirth: false,
            ascension: false,
            unparsed_tooltip_lines: Vec::new(),
        }
    }

    fn build(equipment: Vec<EquippedItem>) -> CharacterBuild {
        CharacterBuild {
            level: 10,
            progression: Progression::Spawn,
            allocated: AttributeAllocation {
                stamina: 0,
                strength: 20,
                agility: 0,
                intellect: 0,
            },
            equipment,
            skills: Vec::new(),
            active_skill_effects: Vec::new(),
            faction_notoriety: None,
            guild_level: None,
        }
    }

    #[test]
    fn equipment_stats_contribute_to_final_requirement_attributes() {
        let items = vec![
            item(
                "helper",
                "Ring",
                ItemStats {
                    strength: Some(10),
                    ..ItemStats::default()
                },
                ItemRequirements::default(),
            ),
            item(
                "sword",
                "One-Hand Sword",
                ItemStats {
                    armor: Some(5),
                    ..ItemStats::default()
                },
                ItemRequirements {
                    strength: Some(50),
                    ..ItemRequirements::default()
                },
            ),
        ];
        let inspection = inspect_build(
            &build(vec![
                EquippedItem {
                    slot: EquipmentSlot::Ring,
                    slot_index: 0,
                    item_slug: "helper".into(),
                },
                EquippedItem {
                    slot: EquipmentSlot::MainHand,
                    slot_index: 0,
                    item_slug: "sword".into(),
                },
            ]),
            &items,
            &[],
            &[],
        );
        assert!(inspection.is_valid());
        assert_eq!(inspection.requirement_attributes.strength, 50);
        assert_eq!(inspection.final_attributes.strength, 50);
        assert_eq!(inspection.item_stats.armor, 5);
        assert_eq!(inspection.confirmed_derived_stats.attack_power, Some(130));
        assert_eq!(inspection.confirmed_derived_stats.armor, 155);
    }

    #[test]
    fn geared_level_103_panel_uses_total_attributes_for_confirmed_stats() {
        let mut aggregate_gear = item(
            "observed-gear",
            "Axe",
            ItemStats {
                armor: Some(120_900),
                stamina: Some(339),
                strength: Some(295),
                agility: Some(647),
                intellect: Some(121),
                attack_power: Some(360),
                crit: Some(56),
                max_health: Some(7_500),
                max_energy: Some(5_000),
                energy_regen: Some(500),
                ..ItemStats::default()
            },
            ItemRequirements::default(),
        );
        aggregate_gear.damage_min = Some(950);
        aggregate_gear.damage_max = Some(1_030);
        aggregate_gear.attack_speed = Some(2.0);
        let mut aggregate_implants = item(
            "observed-implants",
            "Implant",
            ItemStats {
                damage: Some(880),
                ..ItemStats::default()
            },
            ItemRequirements::default(),
        );
        aggregate_implants.implant_slot = Some(ImplantSlot::Brain);
        let set = ItemSet {
            name: "Observed set".into(),
            slug: "observed-set".into(),
            bonus_rule: "Highest eligible tier only".into(),
            resource_tool_rule: "Set bonuses skipped".into(),
            pieces: vec![ItemSetPiece {
                name: aggregate_gear.name.clone(),
                slug: aggregate_gear.slug.clone(),
                item_type: aggregate_gear.item_type.clone(),
                level_requirement: None,
            }],
            bonuses: vec![ItemSetBonus {
                required_pieces: 1,
                effects: vec![
                    ItemSetBonusEffect {
                        stat: "Agility".into(),
                        value: "+30".into(),
                    },
                    ItemSetBonusEffect {
                        stat: "Armor".into(),
                        value: "+2,000".into(),
                    },
                    ItemSetBonusEffect {
                        stat: "Attack power".into(),
                        value: "+80".into(),
                    },
                    ItemSetBonusEffect {
                        stat: "Crit".into(),
                        value: "+5".into(),
                    },
                ],
            }],
        };
        let observed = CharacterBuild {
            level: 103,
            progression: Progression::Ascension,
            allocated: AttributeAllocation {
                stamina: 115,
                strength: 0,
                agility: 176,
                intellect: 0,
            },
            equipment: vec![
                EquippedItem {
                    slot: EquipmentSlot::MainHand,
                    slot_index: 0,
                    item_slug: aggregate_gear.slug.clone(),
                },
                EquippedItem {
                    slot: EquipmentSlot::ImplantBrain,
                    slot_index: 0,
                    item_slug: aggregate_implants.slug.clone(),
                },
            ],
            skills: Vec::new(),
            active_skill_effects: Vec::new(),
            faction_notoriety: None,
            guild_level: None,
        };

        let inspection = inspect_build(
            &observed,
            &[aggregate_gear, aggregate_implants],
            &[set],
            &[],
        );
        assert_eq!(
            inspection.final_attributes,
            AttributeTotals {
                stamina: 474,
                strength: 315,
                agility: 873,
                intellect: 141
            }
        );
        assert_eq!(inspection.confirmed_derived_stats.maximum_health, 18_434);
        assert_eq!(inspection.confirmed_derived_stats.maximum_energy, 8_875);
        assert_eq!(inspection.confirmed_derived_stats.armor, 124_375);
        assert_eq!(inspection.confirmed_derived_stats.attack_power, Some(2_481));
        assert_eq!(
            inspection.confirmed_derived_stats.panel_damage_min,
            Some(2_184)
        );
        assert_eq!(
            inspection.confirmed_derived_stats.panel_damage_max,
            Some(2_264)
        );
        assert_eq!(
            inspection.confirmed_derived_stats.displayed_dodge_percent,
            Some(81.45625)
        );
        assert!(
            (inspection
                .confirmed_derived_stats
                .displayed_crit_percent
                .unwrap()
                - 108.500_714_285_714_28)
                .abs()
                < 1e-9
        );
        assert_eq!(
            inspection
                .confirmed_derived_stats
                .expected_noncritical_basic_attack_hit,
            Some(2_224.0)
        );
        assert!(
            (inspection
                .confirmed_derived_stats
                .expected_basic_attack_dps
                .unwrap()
                - 2_297.955_148_571_428_7)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn speed_buffs_do_not_reduce_damage_per_hit_and_only_implants_add_flat_damage() {
        let mut weapon = item(
            "test-weapon",
            "One-Hand Sword",
            ItemStats::default(),
            ItemRequirements::default(),
        );
        weapon.damage_min = Some(10);
        weapon.damage_max = Some(20);
        weapon.attack_speed = Some(2.0);
        let mut implant = item(
            "test-implant",
            "Implant",
            ItemStats {
                damage: Some(30),
                ..ItemStats::default()
            },
            ItemRequirements::default(),
        );
        implant.implant_slot = Some(ImplantSlot::Brain);
        let speed_buff = Skill {
            name: "Speed buff".into(),
            slug: "speed-buff".into(),
            rank: Some(1),
            quick_facts: Vec::new(),
            level_requirement: None,
            attribute_requirements: Vec::new(),
            cast_time: None,
            duration: None,
            cooldown: None,
            energy_cost: None,
            effects: vec![SkillEffect {
                effect_type: "Timed effect".into(),
                details: "Attack speed -500".into(),
                components: vec![SkillEffectComponent::TimedStatModifier {
                    stat: "Attack speed".into(),
                    value: -500,
                }],
            }],
        };
        let mut character = build(vec![
            EquippedItem {
                slot: EquipmentSlot::MainHand,
                slot_index: 0,
                item_slug: weapon.slug.clone(),
            },
            EquippedItem {
                slot: EquipmentSlot::ImplantBrain,
                slot_index: 0,
                item_slug: implant.slug.clone(),
            },
        ]);
        character.skills.push(speed_buff.slug.clone());
        character.active_skill_effects.push(ActiveSkillEffect {
            skill_slug: speed_buff.slug.clone(),
            role: ActiveSkillRole::Buff,
        });

        let inspection = inspect_build(&character, &[weapon, implant], &[], &[speed_buff]);
        assert_eq!(inspection.confirmed_derived_stats.attack_power, Some(100));
        assert_eq!(
            inspection.confirmed_derived_stats.panel_damage_min,
            Some(54)
        );
        assert_eq!(
            inspection.confirmed_derived_stats.panel_damage_max,
            Some(64)
        );
        assert_eq!(
            inspection.confirmed_derived_stats.attack_interval_seconds,
            1.5
        );
        assert_eq!(
            inspection
                .confirmed_derived_stats
                .expected_noncritical_basic_attack_hit,
            Some(59.0)
        );
    }

    #[test]
    fn invalid_budget_slots_and_non_attribute_requirements_are_reported_together() {
        let items = vec![item(
            "too-strong",
            "Head",
            ItemStats::default(),
            ItemRequirements {
                level: Some(20),
                agility: Some(100),
                ..ItemRequirements::default()
            },
        )];
        let mut invalid = build(vec![
            EquippedItem {
                slot: EquipmentSlot::Ring,
                slot_index: 0,
                item_slug: "too-strong".into(),
            },
            EquippedItem {
                slot: EquipmentSlot::Ring,
                slot_index: 0,
                item_slug: "missing".into(),
            },
        ]);
        invalid.allocated.strength = 21;
        let inspection = inspect_build(&invalid, &items, &[], &[]);
        assert!(!inspection.is_valid());
        assert!(inspection.issues.len() >= 4);
        assert!(
            !inspection
                .issues
                .iter()
                .any(|issue| issue.message.contains("Agility 100"))
        );
    }

    #[test]
    fn morph_only_effects_can_use_buff_slots() {
        let skills = crate::db::load_skills().unwrap();
        let morph = skills
            .iter()
            .find(|skill| skill.slug == "frogget-about-it-129")
            .unwrap();
        assert!(skill_supports_role(morph, ActiveSkillRole::Buff));
        assert!(skill_supports_role(morph, ActiveSkillRole::Morph));
        assert!(!skill_supports_role(morph, ActiveSkillRole::Pet));
    }

    #[test]
    fn attack_skill_recommendations_rank_distinct_locally_castable_skills() {
        let skills = crate::db::load_skills().unwrap();
        let mut build = CharacterBuild {
            level: 104,
            progression: Progression::Ascension,
            allocated: AttributeAllocation {
                stamina: 0,
                strength: 0,
                agility: 314,
                intellect: 0,
            },
            equipment: Vec::new(),
            skills: Vec::new(),
            active_skill_effects: Vec::new(),
            faction_notoriety: None,
            guild_level: None,
        };
        build.skills.push("self-motivated-330".into());
        build.active_skill_effects.push(ActiveSkillEffect {
            skill_slug: "self-motivated-330".into(),
            role: ActiveSkillRole::Buff,
        });
        let inspection = inspect_build(&build, &[], &[], &skills);
        let summary = &inspection.recommended_attack_skills;

        assert_eq!(
            inspection.active_energy_regen.expected_energy_per_second,
            80.0
        );
        assert_eq!(inspection.active_energy_regen.effects.len(), 1);
        assert_eq!(summary.direct_damage_records_evaluated, 40);
        assert_eq!(summary.top.len(), 3);
        assert!(summary.castable_distinct_skills >= summary.top.len());
        assert!(
            summary
                .top
                .windows(2)
                .all(|pair| pair[0].standalone_expected_dps >= pair[1].standalone_expected_dps)
        );
        assert_eq!(
            summary
                .top
                .iter()
                .map(|skill| skill.skill_name.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
        for recommendation in &summary.top {
            assert_eq!(recommendation.energy_regen_per_second, 80.0);
            assert!(
                recommendation.sustained_expected_dps <= recommendation.standalone_expected_dps
            );
            let expected_cast_rate = (1.0 / recommendation.effective_repeat_seconds)
                .min(80.0 / f64::from(recommendation.energy_cost));
            assert!((recommendation.sustained_casts_per_second - expected_cast_rate).abs() < 1e-12);
            assert!(
                (recommendation.sustained_expected_dps
                    - recommendation.expected_damage_per_cast * expected_cast_rate)
                    .abs()
                    < 1e-9
            );
            let skill = skills
                .iter()
                .find(|skill| skill.slug == recommendation.skill_slug)
                .unwrap();
            assert!(
                skill
                    .level_requirement
                    .is_none_or(|level| level <= build.level)
            );
            assert!(
                recommendation.energy_cost
                    <= inspection.confirmed_derived_stats.maximum_energy as u32
            );
            assert!(skill.attribute_requirements.iter().all(|requirement| {
                match requirement.attribute.as_str() {
                    "Agility" => requirement.amount.is_some_and(|amount| {
                        amount <= inspection.requirement_attributes.agility as u32
                    }),
                    "Stamina" | "Strength" | "Intellect" => {
                        requirement.amount.is_some_and(|amount| amount <= 20)
                    }
                    _ => false,
                }
            }));
        }

        build.skills.clear();
        build.active_skill_effects.clear();
        let no_regen = inspect_build(&build, &[], &[], &skills);
        assert_eq!(no_regen.active_energy_regen.expected_energy_per_second, 0.0);
        assert!(
            no_regen
                .recommended_attack_skills
                .top
                .iter()
                .all(|skill| skill.sustained_expected_dps == 0.0)
        );
    }

    #[test]
    fn item_attribute_requirements_allow_overequipping() {
        let overequipped = item(
            "overequipped-ring",
            "Ring",
            ItemStats::default(),
            ItemRequirements {
                strength: Some(999),
                ..ItemRequirements::default()
            },
        );
        let inspection = inspect_build(
            &build(vec![EquippedItem {
                slot: EquipmentSlot::Ring,
                slot_index: 0,
                item_slug: overequipped.slug.clone(),
            }]),
            &[overequipped],
            &[],
            &[],
        );
        assert!(inspection.is_valid());
        assert!(inspection.issues.is_empty());
        assert_eq!(
            inspection.confirmed_derived_stats.displayed_crit_percent,
            Some(6.43)
        );
        assert_eq!(
            inspection.confirmed_derived_stats.displayed_dodge_percent,
            Some(5.0)
        );
    }

    #[test]
    fn confirmed_nude_derived_stats_expose_a_complete_baseline() {
        let ascended = CharacterBuild {
            level: 104,
            progression: Progression::Ascension,
            allocated: AttributeAllocation {
                stamina: 0,
                strength: 0,
                agility: 314,
                intellect: 0,
            },
            equipment: Vec::new(),
            skills: Vec::new(),
            active_skill_effects: Vec::new(),
            faction_notoriety: None,
            guild_level: None,
        };
        let inspection = inspect_build(&ascended, &[], &[], &[]);
        assert!(inspection.is_valid());
        assert_eq!(inspection.allocation_budget, 314);
        assert_eq!(inspection.confirmed_derived_stats.maximum_health, 1_872);
        assert_eq!(inspection.confirmed_derived_stats.maximum_energy, 2_080);
        assert_eq!(inspection.confirmed_derived_stats.armor, 0);
        assert_eq!(inspection.confirmed_derived_stats.attack_power, Some(668));
        assert_eq!(
            inspection.confirmed_derived_stats.panel_damage_min,
            Some(69)
        );
        assert_eq!(
            inspection.confirmed_derived_stats.panel_damage_max,
            Some(72)
        );
        assert_eq!(
            inspection.confirmed_derived_stats.attack_interval_seconds,
            1.4
        );
        assert!(
            (inspection
                .confirmed_derived_stats
                .displayed_crit_percent
                .unwrap()
                - 28.858_571_428_571_43)
                .abs()
                < 1e-9
        );

        assert_eq!(
            inspection.confirmed_derived_stats.displayed_dodge_percent,
            Some(60.875)
        );
        assert_eq!(
            inspection
                .confirmed_derived_stats
                .out_of_combat_health_regen_per_tick_candidate,
            57.0
        );
        assert_eq!(
            inspection
                .confirmed_derived_stats
                .out_of_combat_energy_regen_per_tick_candidate,
            62.0
        );
        assert_eq!(
            inspection
                .confirmed_derived_stats
                .out_of_combat_regen_tick_seconds,
            2
        );
    }

    #[test]
    fn implant_body_part_slots_are_enforced() {
        let mut brain = item(
            "test-brain-implant",
            "Implant",
            ItemStats::default(),
            ItemRequirements::default(),
        );
        brain.implant_slot = Some(ImplantSlot::Brain);
        let mut ascended = build(vec![EquippedItem {
            slot: EquipmentSlot::ImplantHeart,
            slot_index: 0,
            item_slug: "test-brain-implant".into(),
        }]);
        ascended.progression = Progression::Ascension;
        let inspection = inspect_build(&ascended, &[brain], &[], &[]);
        assert!(!inspection.is_valid());
        assert!(inspection.issues.iter().any(|issue| {
            issue
                .message
                .contains("incompatible with slot ImplantHeart")
        }));
    }

    #[test]
    fn every_snapshot_implant_has_exactly_one_body_part_slot() {
        let items: Vec<Item> =
            serde_json::from_slice(&std::fs::read("data/items.json").expect("item snapshot"))
                .expect("valid item snapshot");
        let implants: Vec<_> = items
            .iter()
            .filter(|item| item.item_type == "Implant")
            .collect();
        let unclassified: Vec<_> = implants
            .iter()
            .filter(|item| item.implant_slot.is_none())
            .map(|item| item.slug.as_str())
            .collect();
        assert_eq!(implants.len(), 68);
        assert!(unclassified.is_empty());
        assert_eq!(
            implants
                .iter()
                .find(|item| item.slug == "sacred-gauntlet-implant-1492")
                .and_then(|item| item.implant_slot),
            Some(ImplantSlot::LeftArm)
        );
    }

    #[test]
    fn skills_validate_requirements_and_typed_active_roles() {
        let buff = Skill {
            name: "Buff".into(),
            slug: "buff".into(),
            rank: None,
            quick_facts: Vec::new(),
            level_requirement: Some(10),
            attribute_requirements: vec![SkillAttributeRequirement {
                attribute: "Strength".into(),
                amount: Some(100),
                value: "100 Strength".into(),
            }],
            cast_time: None,
            duration: None,
            cooldown: None,
            energy_cost: None,
            effects: vec![SkillEffect {
                effect_type: "Buff".into(),
                details: String::new(),
                components: vec![SkillEffectComponent::TimedStatModifier {
                    stat: "Crit".into(),
                    value: 2,
                }],
            }],
        };
        let mut selected = build(Vec::new());
        selected.skills.push("buff".into());
        selected.active_skill_effects.push(ActiveSkillEffect {
            skill_slug: "buff".into(),
            role: ActiveSkillRole::Buff,
        });
        let skills = vec![buff];
        let inspection = inspect_build(&selected, &[], &[], &skills);
        assert!(inspection.is_valid());
        assert_eq!(inspection.skills[0].skill_name, "Buff");
        assert_eq!(inspection.skills[0].rank, None);
        assert_eq!(
            inspection.skills[0].active_roles,
            vec![ActiveSkillRole::Buff]
        );
        assert_eq!(inspection.active_effect_stats.crit, 2);
        assert_eq!(
            inspection.confirmed_derived_stats.displayed_crit_percent,
            Some(8.43)
        );

        selected.active_skill_effects[0].role = ActiveSkillRole::Pet;
        let inspection = inspect_build(&selected, &[], &[], &skills);
        assert!(!inspection.is_valid());
    }

    #[test]
    fn pets_and_attack_skills_require_attributes_while_external_morphs_do_not() {
        fn required_skill(slug: &str, component: SkillEffectComponent) -> Skill {
            Skill {
                name: slug.into(),
                slug: slug.into(),
                rank: None,
                quick_facts: Vec::new(),
                level_requirement: None,
                attribute_requirements: vec![SkillAttributeRequirement {
                    attribute: "Intellect".into(),
                    amount: Some(100),
                    value: "100 Intellect".into(),
                }],
                cast_time: None,
                duration: None,
                cooldown: None,
                energy_cost: None,
                effects: vec![SkillEffect {
                    effect_type: "Test".into(),
                    details: String::new(),
                    components: vec![component],
                }],
            }
        }

        let skills = vec![
            required_skill("morph", SkillEffectComponent::Morph),
            required_skill("pet", SkillEffectComponent::Pet),
            required_skill(
                "attack",
                SkillEffectComponent::DirectDamage {
                    amount: EffectAmount::Fixed(10),
                },
            ),
        ];
        let mut selected = build(Vec::new());
        selected.skills = vec!["morph".into(), "pet".into(), "attack".into()];
        selected.active_skill_effects = vec![
            ActiveSkillEffect {
                skill_slug: "morph".into(),
                role: ActiveSkillRole::Morph,
            },
            ActiveSkillEffect {
                skill_slug: "pet".into(),
                role: ActiveSkillRole::Pet,
            },
        ];

        let inspection = inspect_build(&selected, &[], &[], &skills);
        let requirement_errors: Vec<_> = inspection
            .issues
            .iter()
            .filter(|issue| issue.message.contains("requires Intellect 100"))
            .map(|issue| issue.source_slug.as_deref())
            .collect();
        assert_eq!(requirement_errors, vec![Some("pet"), Some("attack")]);
    }

    #[test]
    fn active_buff_stats_share_the_crit_soft_cap_and_pet_rules_are_enforced() {
        fn active_skill(slug: &str, role: ActiveSkillRole, modifiers: &[(&str, i32)]) -> Skill {
            let mut components: Vec<_> = modifiers
                .iter()
                .map(|(stat, value)| SkillEffectComponent::TimedStatModifier {
                    stat: (*stat).into(),
                    value: *value,
                })
                .collect();
            if role == ActiveSkillRole::Pet {
                components.push(SkillEffectComponent::Pet);
            }
            Skill {
                name: slug.into(),
                slug: slug.into(),
                rank: Some(2),
                quick_facts: Vec::new(),
                level_requirement: None,
                attribute_requirements: Vec::new(),
                cast_time: None,
                duration: None,
                cooldown: None,
                energy_cost: None,
                effects: vec![SkillEffect {
                    effect_type: format!("{role:?}"),
                    details: String::new(),
                    components,
                }],
            }
        }

        let mut skills: Vec<_> = (0..6)
            .map(|index| active_skill(&format!("buff-{index}"), ActiveSkillRole::Buff, &[]))
            .collect();
        skills.push(active_skill(
            "crit-buff",
            ActiveSkillRole::Buff,
            &[("Crit", 100), ("Agility", 20), ("Attack power", 10)],
        ));
        skills.push(active_skill(
            "pet",
            ActiveSkillRole::Pet,
            &[("Intellect", 10), ("Crit", 5)],
        ));

        let mut selected = build(Vec::new());
        selected.skills = vec!["crit-buff".into()];
        selected.active_skill_effects = vec![ActiveSkillEffect {
            skill_slug: "crit-buff".into(),
            role: ActiveSkillRole::Buff,
        }];
        let inspection = inspect_build(&selected, &[], &[], &skills);
        assert!(inspection.is_valid());
        assert_eq!(inspection.active_effect_stats.agility, 20);
        assert_eq!(inspection.active_effect_stats.attack_power, 10);
        assert_eq!(inspection.active_effect_stats.crit, 100);
        assert_eq!(inspection.final_attributes.agility, 40);
        assert_eq!(
            inspection.confirmed_derived_stats.displayed_crit_percent,
            Some(displayed_crit_from_total_attributes(40, 20, 100))
        );

        selected.skills = (0..6).map(|index| format!("buff-{index}")).collect();
        selected.active_skill_effects = selected
            .skills
            .iter()
            .map(|slug| ActiveSkillEffect {
                skill_slug: slug.clone(),
                role: ActiveSkillRole::Buff,
            })
            .collect();
        let inspection = inspect_build(&selected, &[], &[], &skills);
        assert!(
            inspection
                .issues
                .iter()
                .any(|issue| issue.message.contains("at most 5 buffs"))
        );

        selected.skills = vec!["pet".into()];
        selected.active_skill_effects = vec![ActiveSkillEffect {
            skill_slug: "pet".into(),
            role: ActiveSkillRole::Pet,
        }];
        let inspection = inspect_build(&selected, &[], &[], &skills);
        assert!(
            inspection
                .issues
                .iter()
                .any(|issue| issue.message.contains("require Rebirth"))
        );

        selected.progression = Progression::Rebirth;
        let inspection = inspect_build(&selected, &[], &[], &skills);
        assert!(inspection.is_valid());
        assert_eq!(inspection.active_effect_stats.intellect, 10);
        assert_eq!(inspection.active_effect_stats.crit, 5);
    }

    #[test]
    fn highest_set_tier_counts_duplicate_pieces_and_tools_suppress_it() {
        let ring = item(
            "set-ring",
            "Ring",
            ItemStats::default(),
            ItemRequirements::default(),
        );
        let set = ItemSet {
            name: "Set".into(),
            slug: "set".into(),
            bonus_rule: String::new(),
            resource_tool_rule: String::new(),
            pieces: vec![ItemSetPiece {
                name: "Ring".into(),
                slug: "set-ring".into(),
                item_type: "Ring".into(),
                level_requirement: None,
            }],
            bonuses: vec![
                ItemSetBonus {
                    required_pieces: 1,
                    effects: vec![ItemSetBonusEffect {
                        stat: "Armor".into(),
                        value: "+1".into(),
                    }],
                },
                ItemSetBonus {
                    required_pieces: 2,
                    effects: vec![ItemSetBonusEffect {
                        stat: "Armor".into(),
                        value: "+2".into(),
                    }],
                },
            ],
        };
        let equipped = vec![
            EquippedItem {
                slot: EquipmentSlot::Ring,
                slot_index: 0,
                item_slug: "set-ring".into(),
            },
            EquippedItem {
                slot: EquipmentSlot::Ring,
                slot_index: 1,
                item_slug: "set-ring".into(),
            },
        ];
        let tool = item(
            "pickaxe",
            "Mining Tool",
            ItemStats::default(),
            ItemRequirements::default(),
        );
        let items = vec![ring, tool];
        let sets = vec![set];
        let inspection = inspect_build(&build(equipped.clone()), &items, &sets, &[]);
        assert_eq!(inspection.active_set_tiers[0].set_name, "Set");
        assert_eq!(inspection.active_set_tiers[0].required_pieces, 2);
        assert_eq!(inspection.set_bonus_stats.armor, 2);

        let mut with_tool = equipped;
        with_tool.push(EquippedItem {
            slot: EquipmentSlot::MainHand,
            slot_index: 0,
            item_slug: "pickaxe".into(),
        });
        let inspection = inspect_build(&build(with_tool), &items, &sets, &[]);
        assert!(inspection.active_set_tiers.is_empty());
        assert_eq!(inspection.set_bonus_stats, AggregatedItemStats::default());
    }

    #[test]
    fn every_snapshot_set_effect_has_a_typed_aggregate_destination() {
        let sets: Vec<ItemSet> = serde_json::from_slice(
            &std::fs::read("data/item-sets.json").expect("item-set snapshot"),
        )
        .expect("valid item-set snapshot");
        for effect in sets
            .iter()
            .flat_map(|set| &set.bonuses)
            .flat_map(|tier| &tier.effects)
        {
            let mut totals = AggregatedItemStats::default();
            add_set_effect(&mut totals, effect).unwrap_or_else(|error| panic!("{error}"));
        }
    }
}
