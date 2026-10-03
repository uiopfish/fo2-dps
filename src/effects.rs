//! Conservative normalization of verified wiki effect text, without combat formulas.

use crate::db::{EffectAmount, EffectDirection, EffectResource, SkillEffectComponent};

/// Replaces derived components from raw text, preserving effect_type and details.
/// Repeated calls produce the same components rather than appending duplicates.
pub fn normalize_skill_effects(skill: &mut crate::db::Skill) {
    for effect in &mut skill.effects {
        effect.components = effect
            .details
            .split(" · ")
            .map(|text| {
                parse_component(&effect.effect_type, text).unwrap_or_else(|| {
                    SkillEffectComponent::Unknown {
                        text: text.to_owned(),
                    }
                })
            })
            .collect();
    }
}

// Only accept plain digits or correctly grouped thousands, never silently strip
// arbitrary punctuation from an otherwise unknown fragment.
fn number(text: &str) -> Option<u32> {
    let groups: Vec<_> = text.split(',').collect();
    if groups
        .iter()
        .any(|g| g.is_empty() || !g.bytes().all(|b| b.is_ascii_digit()))
        || (groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3)))
    {
        return None;
    }
    groups.concat().parse().ok()
}

fn amount(text: &str) -> Option<EffectAmount> {
    if let Some((min, max)) = text.split_once('–') {
        Some(EffectAmount::Range {
            min: number(min)?,
            max: number(max)?,
        })
    } else {
        Some(EffectAmount::Fixed(number(text)?))
    }
}

fn parse_component(kind: &str, text: &str) -> Option<SkillEffectComponent> {
    use SkillEffectComponent::*;
    match kind {
        "Instant damage" => Some(DirectDamage {
            amount: amount(text.strip_suffix(" damage")?)?,
        }),
        "Healing" => Some(DirectHeal {
            amount: amount(text.strip_suffix(" health")?)?,
        }),
        "Health loss" => Some(HealthLoss {
            amount: amount(text.strip_suffix(" health")?)?,
        }),
        "Threat" => Some(ThreatMultiplier {
            multiplier: number(
                text.strip_prefix("Generates ")?
                    .strip_suffix("× threat from the current damage roll.")?,
            )?,
        }),
        "Teleport" if text == "Teleports the caster." => Some(Teleport),
        "Timed effect" => timed_component(text),
        _ => None,
    }
}

fn timed_component(text: &str) -> Option<SkillEffectComponent> {
    use SkillEffectComponent::*;
    match text {
        "Pet effect" => return Some(Pet),
        "Morph effect" => return Some(Morph),
        _ => {}
    }

    if let Some((change, interval)) = text.split_once(" every ") {
        let interval_seconds = number(interval.strip_suffix(" seconds")?)?;
        if interval_seconds == 0 {
            return None;
        }
        // These resource/action pairs are the exact wording present in skills.json.
        let (value, resource, direction) =
            if let Some(value) = change.strip_suffix(" health restored") {
                (value, EffectResource::Health, EffectDirection::Gain)
            } else if let Some(value) = change.strip_suffix(" health lost") {
                (value, EffectResource::Health, EffectDirection::Loss)
            } else if let Some(value) = change.strip_suffix(" energy gained") {
                (value, EffectResource::Energy, EffectDirection::Gain)
            } else if let Some(value) = change.strip_suffix(" energy lost") {
                (value, EffectResource::Energy, EffectDirection::Loss)
            } else {
                return None;
            };
        return Some(Periodic {
            resource,
            direction,
            amount: amount(value)?,
            interval_seconds,
        });
    }

    let (stat, value) = text.rsplit_once(' ')?;
    if !matches!(
        stat,
        "Agility"
            | "Armor"
            | "Attack power"
            | "Attack speed"
            | "Crit"
            | "Damage absorption"
            | "Intellect"
            | "Max health"
            | "Move speed"
            | "Stamina"
            | "Strength"
    ) {
        return None;
    }
    let value = if let Some(value) = value.strip_prefix('+') {
        i32::try_from(number(value)?).ok()?
    } else if let Some(value) = value.strip_prefix('-') {
        i32::try_from(-i64::from(number(value)?)).ok()?
    } else if stat == "Move speed" && value == "0" {
        0
    } else {
        return None;
    };
    Some(TimedStatModifier {
        stat: stat.to_owned(),
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Skill, SkillEffect};
    use SkillEffectComponent::*;

    #[test]
    fn direct_amounts_preserve_literal_ranges() {
        assert_eq!(
            parse_component("Instant damage", "1,250–2,250 damage"),
            Some(DirectDamage {
                amount: EffectAmount::Range {
                    min: 1250,
                    max: 2250
                },
            })
        );
        assert_eq!(
            parse_component("Healing", "100 health"),
            Some(DirectHeal {
                amount: EffectAmount::Fixed(100)
            })
        );
        assert_eq!(
            parse_component("Health loss", "4,000 health"),
            Some(HealthLoss {
                amount: EffectAmount::Fixed(4000)
            })
        );
        assert_eq!(
            amount("20–10"),
            Some(EffectAmount::Range { min: 20, max: 10 })
        );
    }

    #[test]
    fn timed_values_and_flags() {
        assert_eq!(
            timed_component("Attack speed -300"),
            Some(TimedStatModifier {
                stat: "Attack speed".into(),
                value: -300
            })
        );
        assert_eq!(
            timed_component("Armor +10,000"),
            Some(TimedStatModifier {
                stat: "Armor".into(),
                value: 10000
            })
        );
        assert_eq!(
            timed_component("Move speed 0"),
            Some(TimedStatModifier {
                stat: "Move speed".into(),
                value: 0
            })
        );
        for (text, resource, direction, expected_amount, interval_seconds) in [
            (
                "100–350 health restored every 5 seconds",
                EffectResource::Health,
                EffectDirection::Gain,
                EffectAmount::Range { min: 100, max: 350 },
                5,
            ),
            (
                "20–40 health lost every 2 seconds",
                EffectResource::Health,
                EffectDirection::Loss,
                EffectAmount::Range { min: 20, max: 40 },
                2,
            ),
            (
                "3,500 energy gained every 3 seconds",
                EffectResource::Energy,
                EffectDirection::Gain,
                EffectAmount::Fixed(3500),
                3,
            ),
            (
                "100 energy lost every 8 seconds",
                EffectResource::Energy,
                EffectDirection::Loss,
                EffectAmount::Fixed(100),
                8,
            ),
        ] {
            assert_eq!(
                timed_component(text),
                Some(Periodic {
                    resource,
                    direction,
                    amount: expected_amount,
                    interval_seconds
                })
            );
        }
        assert_eq!(timed_component("Pet effect"), Some(Pet));
        assert_eq!(timed_component("Morph effect"), Some(Morph));
        assert_eq!(
            parse_component("Teleport", "Teleports the caster."),
            Some(Teleport)
        );
        assert_eq!(
            parse_component(
                "Threat",
                "Generates 20× threat from the current damage roll."
            ),
            Some(ThreatMultiplier { multiplier: 20 })
        );
    }

    #[test]
    fn rejects_unverified_or_malformed_text() {
        for text in [
            "Armor +1,00",
            "Armor +10%",
            "Luck +10",
            "Armor 10",
            "Pet effect!",
            "10 health gained every 5 seconds",
            "10 energy gained every 0 seconds",
            "10 energy gained every 5 seconds extra",
        ] {
            assert_eq!(timed_component(text), None, "{text}");
        }
        assert_eq!(parse_component("Other", "100 health"), None);
        assert_eq!(parse_component("Healing", "10-20 health"), None);
        assert_eq!(number("4294967296"), None);
    }

    #[test]
    fn legacy_deserialization_unknown_fragments_and_idempotence() {
        let mut skills: Vec<Skill> =
            serde_json::from_str(include_str!("../data/skills.json")).unwrap();
        let skill = &mut skills[0];
        let effect: SkillEffect = serde_json::from_str(
            r#"{"effect_type":"Timed effect","details":"Armor +300 · mystery · Pet effect"}"#,
        )
        .unwrap();
        assert!(effect.components.is_empty());
        skill.effects = vec![effect];
        normalize_skill_effects(skill);
        assert_eq!(
            skill.effects[0].components,
            vec![
                TimedStatModifier {
                    stat: "Armor".into(),
                    value: 300
                },
                Unknown {
                    text: "mystery".into()
                },
                Pet,
            ]
        );
        let first = serde_json::to_value(&*skill).unwrap();
        normalize_skill_effects(skill);
        assert_eq!(first, serde_json::to_value(&*skill).unwrap());
        assert_eq!(skill.effects[0].effect_type, "Timed effect");
        assert_eq!(
            skill.effects[0].details,
            "Armor +300 · mystery · Pet effect"
        );
        let roundtrip: Skill = serde_json::from_value(first).unwrap();
        assert_eq!(roundtrip.effects[0].components, skill.effects[0].components);
    }

    #[test]
    fn dataset_has_only_the_explicitly_unsupported_energy_transfer() {
        let mut skills: Vec<Skill> =
            serde_json::from_str(include_str!("../data/skills.json")).unwrap();
        let mut unknown = std::collections::BTreeSet::new();
        for skill in &mut skills {
            normalize_skill_effects(skill);
            for effect in &skill.effects {
                assert!(!effect.components.is_empty());
                for component in &effect.components {
                    if let Unknown { text } = component {
                        unknown.insert((effect.effect_type.clone(), text.clone()));
                    }
                }
            }
        }
        assert_eq!(
            unknown,
            std::collections::BTreeSet::from([(
                "Energy transfer".into(),
                "Transfers 1,000 energy.".into()
            ),])
        );
    }
}
