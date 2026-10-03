#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Item {
    pub name: String,
    pub slug: String,
    pub item_type: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub implant_slot: Option<ImplantSlot>,

    pub damage_min: Option<u32>,
    pub damage_max: Option<u32>,
    pub attack_speed: Option<f32>,

    pub stats: ItemStats,
    pub requirements: ItemRequirements,

    pub market_price: Option<u64>,
    pub recently_sold_price: Option<u64>,
    pub shop_price: Option<u64>,
    pub sellable_to_shops: bool,

    pub rebirth: bool,
    #[serde(default)]
    pub ascension: bool,
    pub unparsed_tooltip_lines: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ImplantSlot {
    Brain,
    Heart,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

impl ImplantSlot {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Brain => "brain",
            Self::Heart => "heart",
            Self::LeftArm => "left-arm",
            Self::RightArm => "right-arm",
            Self::LeftLeg => "left-leg",
            Self::RightLeg => "right-leg",
        }
    }

    pub fn from_description(description: &str) -> Option<Self> {
        match description.trim() {
            "Implant for your Brain." => Some(Self::Brain),
            "Implant for your Heart." => Some(Self::Heart),
            "Implant for your Left Arm." => Some(Self::LeftArm),
            "Implant for your Right Arm." => Some(Self::RightArm),
            "Implant for your Left Leg." => Some(Self::LeftLeg),
            "Implant for your Right Leg." => Some(Self::RightLeg),
            _ => None,
        }
    }
}

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct ItemStats {
    pub armor: Option<i32>,
    pub stamina: Option<i32>,
    pub strength: Option<i32>,
    pub agility: Option<i32>,
    pub intellect: Option<i32>,
    pub attack_power: Option<i32>,
    pub crit: Option<i32>,
    pub damage: Option<i32>,
    pub cast_time_reduction: Option<i32>,
    pub max_health: Option<i32>,
    pub max_energy: Option<i32>,
    pub health_regen: Option<i32>,
    pub energy_regen: Option<i32>,
}

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct ItemRequirements {
    pub level: Option<u32>,
    pub stamina: Option<u32>,
    pub strength: Option<u32>,
    pub agility: Option<u32>,
    pub intellect: Option<u32>,
    pub faction_notoriety: Option<u32>,
    pub guild_level: Option<u32>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ItemSet {
    pub name: String,
    pub slug: String,
    pub bonus_rule: String,
    pub resource_tool_rule: String,
    pub pieces: Vec<ItemSetPiece>,
    pub bonuses: Vec<ItemSetBonus>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ItemSetPiece {
    pub name: String,
    pub slug: String,
    pub item_type: String,
    pub level_requirement: Option<u32>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ItemSetBonus {
    pub required_pieces: u32,
    pub effects: Vec<ItemSetBonusEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ItemSetBonusEffect {
    pub stat: String,
    pub value: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Skill {
    pub name: String,
    pub slug: String,
    pub rank: Option<u32>,
    pub quick_facts: Vec<SkillQuickFact>,
    pub level_requirement: Option<u32>,
    pub attribute_requirements: Vec<SkillAttributeRequirement>,
    pub cast_time: Option<f32>,
    pub duration: Option<SkillDuration>,
    pub cooldown: Option<SkillCooldown>,
    pub energy_cost: Option<u32>,
    pub effects: Vec<SkillEffect>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_skills() -> anyhow::Result<Vec<Skill>> {
    use anyhow::Context;
    use std::collections::BTreeSet;

    let mut skills: Vec<Skill> = serde_json::from_slice(&std::fs::read("data/skills.json")?)
        .context("Could not load data/skills.json")?;
    let supplemental: Vec<Skill> =
        serde_json::from_slice(&std::fs::read("data/skills-supplemental.json")?)
            .context("Could not load data/skills-supplemental.json")?;
    let mut slugs = BTreeSet::new();
    for skill in skills.iter().chain(&supplemental) {
        anyhow::ensure!(
            slugs.insert(skill.slug.as_str()),
            "Duplicate skill slug across canonical and supplemental data: {}",
            skill.slug
        );
    }
    skills.extend(supplemental);
    Ok(skills)
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SkillQuickFact {
    pub label: String,
    pub value: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SkillAttributeRequirement {
    pub attribute: String,
    pub amount: Option<u32>,
    pub value: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum SkillDuration {
    NoTimedDuration,
    Seconds(f32),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum SkillCooldown {
    None,
    Seconds(f32),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SkillEffect {
    pub effect_type: String,
    pub details: String,
    #[serde(default)]
    pub components: Vec<SkillEffectComponent>,
}

/// Literal endpoints from the source; no endpoint inclusivity or roll formula is implied.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum EffectAmount {
    Fixed(u32),
    Range { min: u32, max: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum EffectResource {
    Health,
    Energy,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum EffectDirection {
    Gain,
    Loss,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum SkillEffectComponent {
    DirectDamage {
        amount: EffectAmount,
    },
    DirectHeal {
        amount: EffectAmount,
    },
    HealthLoss {
        amount: EffectAmount,
    },
    /// Uses the containing skill's duration; stat units are not inferred.
    TimedStatModifier {
        stat: String,
        value: i32,
    },
    Periodic {
        resource: EffectResource,
        direction: EffectDirection,
        amount: EffectAmount,
        interval_seconds: u32,
    },
    /// Multiplies threat from the current damage roll, not damage itself.
    ThreatMultiplier {
        multiplier: u32,
    },
    Teleport,
    Pet,
    Morph,
    Unknown {
        text: String,
    },
}

#[derive(Debug, serde::Serialize)]
pub struct ScrapeReport {
    pub entity: String,
    pub discovered: usize,
    pub resumed: usize,
    pub scraped: usize,
    pub failures: Vec<ScrapeFailure>,
    pub unparsed_item_tooltip_lines: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct ScrapeFailure {
    pub slug: String,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taught_pet(description: &str) -> Option<(String, u32)> {
        let taught = description.strip_prefix("Teaches Pet ")?;
        let (name, rank_and_description) = taught.split_once(" Rank ")?;
        let rank = rank_and_description.split_once('.')?.0.parse().ok()?;
        Some((format!("Pet {name}"), rank))
    }

    fn components(skill: &Skill) -> impl Iterator<Item = &SkillEffectComponent> {
        skill.effects.iter().flat_map(|effect| &effect.components)
    }

    #[test]
    fn every_pet_teaching_item_has_a_selectable_pet_record() {
        let items: Vec<Item> = serde_json::from_str(include_str!("../data/items.json")).unwrap();
        let skills = load_skills().unwrap();
        let mut taught = 0;

        for item in &items {
            let Some((name, rank)) = item.description.as_deref().and_then(taught_pet) else {
                continue;
            };
            taught += 1;
            let skill = skills
                .iter()
                .find(|skill| skill.name == name && skill.rank == Some(rank))
                .unwrap_or_else(|| panic!("missing {name} Rank {rank}, taught by {}", item.slug));
            assert!(
                components(skill).any(|component| matches!(component, SkillEffectComponent::Pet)),
                "{} is not selectable as a pet",
                skill.slug
            );
        }

        assert!(
            taught > 20,
            "pet coverage test found suspiciously few teaching items"
        );
    }

    #[test]
    fn supplemental_pets_preserve_source_requirements_and_descriptions() {
        let items: Vec<Item> = serde_json::from_str(include_str!("../data/items.json")).unwrap();
        let supplemental: Vec<Skill> =
            serde_json::from_str(include_str!("../data/skills-supplemental.json")).unwrap();

        for skill in &supplemental {
            let item = items.iter().find(|item| item.slug == skill.slug).unwrap();
            assert_eq!(
                skill.level_requirement, item.requirements.level,
                "{}",
                skill.slug
            );
            for (attribute, amount) in [
                ("Stamina", item.requirements.stamina),
                ("Strength", item.requirements.strength),
                ("Agility", item.requirements.agility),
                ("Intellect", item.requirements.intellect),
            ] {
                assert_eq!(
                    skill
                        .attribute_requirements
                        .iter()
                        .find(|requirement| requirement.attribute == attribute)
                        .and_then(|requirement| requirement.amount),
                    amount,
                    "{} {attribute}",
                    skill.slug
                );
            }
            assert_eq!(
                skill.attribute_requirements.iter().any(|requirement| {
                    requirement.attribute == "Progression" && requirement.value == "Rebirth"
                }),
                item.rebirth,
                "{} progression",
                skill.slug
            );

            let description = item.description.as_deref().unwrap();
            let published_effect_text = description.split_once(". ").unwrap().1;
            assert_eq!(
                skill.effects[0].details, published_effect_text,
                "{}",
                skill.slug
            );
        }
    }

    #[test]
    fn supplemental_pet_components_capture_published_build_stats() {
        let skills: Vec<Skill> =
            serde_json::from_str(include_str!("../data/skills-supplemental.json")).unwrap();
        let skill = |slug: &str| skills.iter().find(|skill| skill.slug == slug).unwrap();

        assert!(
            components(skill("pet-goldfish-egg-3002")).any(|component| matches!(
                component,
                SkillEffectComponent::TimedStatModifier { stat, value }
                    if stat == "Attack speed" && *value == -250
            ))
        );
        assert!(
            components(skill("uber-pet-hot-potato-egg-3235")).any(|component| matches!(
                component,
                SkillEffectComponent::TimedStatModifier { stat, value }
                    if stat == "Attack power" && *value == 1300
            ))
        );
        assert!(
            components(skill("uber-pet-hot-potato-egg-3235")).any(|component| matches!(
                component,
                SkillEffectComponent::TimedStatModifier { stat, value }
                    if stat == "Crit" && *value == 14
            ))
        );
        assert!(
            components(skill("uber-pet-tortoise-egg-3007")).any(|component| matches!(
                component,
                SkillEffectComponent::TimedStatModifier { stat, value }
                    if stat == "Armor" && *value == 20_000
            ))
        );
        assert!(
            components(skill("uber-pet-zombie-egg-2847")).any(|component| matches!(
                component,
                SkillEffectComponent::Periodic {
                    resource: EffectResource::Health,
                    direction: EffectDirection::Gain,
                    amount: EffectAmount::Range { min: 100, max: 700 },
                    interval_seconds: 5,
                }
            ))
        );

        let chicken = skill("pet-black-chicken-egg-2467");
        assert!(matches!(
            chicken.duration,
            Some(SkillDuration::Seconds(seconds)) if seconds == 1_800.0
        ));
        assert!(
            chicken.effects[0]
                .details
                .contains("Crosses roads for loot, not questions.")
        );

        let hot_potato = skill("pet-hot-potato-egg-3234");
        assert!(matches!(
            hot_potato.duration,
            Some(SkillDuration::Seconds(seconds)) if seconds == 1_800.0
        ));
        assert!(hot_potato.effects[0].details.contains("for 30 minutes."));
    }
}
