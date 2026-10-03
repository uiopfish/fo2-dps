//! Mob data as published by the database; no combat or reward calculations.
//!
//! Uses the shared retrying HTTP client and pagination-aware discovery.
//! Optional typed values mean absent/unrecognized, not zero. Raw data is kept
//! even when a value cannot be interpreted. No boss status is inferred.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mob {
    pub slug: String,
    pub name: String,
    pub source_url: String,
    pub level: Option<u64>,
    pub health: Option<u64>,
    pub damage: Option<MobRange>,
    pub attack_speed_ms: Option<u64>,
    /// `Some(false)` for the explicit "Does not attack" label.
    pub attacks: Option<bool>,
    pub faction: Option<MobValue>,
    pub faction_xp: Option<i64>,
    /// Preserve the published wording, including "No enforced weapon type".
    pub required_weapon: Option<MobValue>,
    pub aggressive: Option<bool>,
    pub debuff_skill_count: Option<u64>,
    /// Includes unlinked entries such as "Unlisted debuff".
    pub debuffs: Vec<MobValue>,
    pub locations: Vec<MobLocation>,
    pub drop_profiles: Vec<MobDropProfile>,
    pub raw: MobRawPage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobRange {
    pub min: u64,
    pub max: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobLink {
    /// Original href, including all query and fragment parameters.
    pub href: String,
    pub text: String,
    pub aria_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobValue {
    pub text: String,
    pub links: Vec<MobLink>,
    /// Retains nested roll boundaries, attributes, images and line breaks.
    pub html: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobFact {
    pub label: String,
    pub value: MobValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobTable {
    pub caption: Option<String>,
    pub aria_label: Option<String>,
    pub headers: Vec<MobValue>,
    pub rows: Vec<Vec<MobValue>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobSection {
    pub id: Option<String>,
    pub heading: Option<String>,
    pub facts: Vec<MobFact>,
    pub tables: Vec<MobTable>,
    pub content: MobValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobRawPage {
    pub facts: Vec<MobFact>,
    /// All sections/asides, not just the currently understood ones.
    pub sections: Vec<MobSection>,
    pub tables: Vec<MobTable>,
    pub links: Vec<MobLink>,
    /// Original response, including structured metadata and update dates.
    /// This is the lossless fallback for new fields and unrecognized markup.
    pub source_html: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobLocation {
    pub zone: MobLink,
    /// Published map-location count, not a spawn count or respawn rate.
    pub map_location_count: Option<u64>,
    pub raw: MobValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MobDropProfile {
    pub summary: MobValue,
    pub zone: Option<MobValue>,
    pub map_location_count: Option<u64>,
    /// Inclusive endpoints as displayed for one eligible solo looter.
    pub solo_coins: Option<MobRange>,
    pub drops: Vec<MobDrop>,
    pub facts: Vec<MobFact>,
    pub tables: Vec<MobTable>,
    /// Retains no-drop notices, caveats and any additional profile rules.
    pub raw: MobValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MobDrop {
    pub item: MobValue,
    /// Ordered independent rolls; intentionally never deduplicated.
    pub rolls: Vec<MobDropRoll>,
    /// Displayed percentage (0–100), not a probability or a computed value.
    /// Calculator links may contain more precision and remain in raw_cells.
    pub solo_chance_at_least_one_percent: Option<f64>,
    pub maximum_quantity: Option<u64>,
    pub raw_cells: Vec<MobValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MobDropRoll {
    /// Published source, e.g. Spawn, Global or Zone; not a closed enum.
    pub source: Option<String>,
    pub chance_percent: Option<f64>,
    pub raw: MobValue,
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use anyhow::{Context, Result, ensure};
    use scraper::{ElementRef, Html, Selector};

    const BASE_URL: &str = "https://db.fantasyonline2.com";

    pub fn fetch_mob(slug: &str) -> Result<Mob> {
        // Reject paths/queries before constructing a URL, without assuming IDs
        // will always use today's numeric suffix convention.
        ensure!(
            !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "Invalid mob slug: {slug}"
        );
        let url = format!("{BASE_URL}/mobs/{slug}");
        println!("Fetching {url}");
        let html = crate::scraper::fetch_page(&url)
            .with_context(|| format!("Could not fetch mob: {url}"))?;
        parse_mob(slug, &html).with_context(|| format!("Could not parse mob: {url}"))
    }

    pub fn fetch_mob_slugs() -> Result<Vec<String>> {
        crate::scraper::fetch_index_slugs("mobs")
    }

    fn selector(css: &str) -> Selector {
        Selector::parse(css).expect("static mob selector")
    }

    fn text(element: ElementRef<'_>) -> String {
        // Concatenate before normalizing: React splits numeric values and their
        // units into separate text nodes (sometimes separated by comments).
        element
            .text()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn first<'a>(element: ElementRef<'a>, css: &str) -> Option<ElementRef<'a>> {
        element.select(&selector(css)).next()
    }

    fn link(element: ElementRef<'_>) -> MobLink {
        MobLink {
            href: element.value().attr("href").unwrap_or_default().to_string(),
            text: text(element),
            aria_label: element.value().attr("aria-label").map(str::to_string),
        }
    }

    fn value(element: ElementRef<'_>) -> MobValue {
        let mut links = Vec::new();
        if element.value().name() == "a" && element.value().attr("href").is_some() {
            links.push(link(element));
        }
        links.extend(element.select(&selector("a[href]")).map(link));
        MobValue {
            text: text(element),
            links,
            html: element.html(),
        }
    }

    fn facts(element: ElementRef<'_>) -> Vec<MobFact> {
        let mut result = Vec::new();
        for dt in element.select(&selector("dt")) {
            let mut sibling = dt.next_sibling();
            while let Some(node) = sibling {
                sibling = node.next_sibling();
                let Some(dd) = ElementRef::wrap(node) else {
                    continue;
                };
                if dd.value().name() == "dt" {
                    break;
                }
                if dd.value().name() == "dd" {
                    result.push(MobFact {
                        label: text(dt),
                        value: value(dd),
                    });
                }
            }
        }
        // Combat traits use span/strong pairs instead of definition lists.
        for strong in element.select(&selector("div > strong")) {
            let Some(parent) = strong.parent().and_then(ElementRef::wrap) else {
                continue;
            };
            if let Some(label) = parent
                .children()
                .filter_map(ElementRef::wrap)
                .find(|child| child.value().name() == "span")
            {
                result.push(MobFact {
                    label: text(label),
                    value: value(strong),
                });
            }
        }
        result
    }

    fn fact<'a>(facts: &'a [MobFact], label: &str) -> Option<&'a MobValue> {
        facts
            .iter()
            .find(|fact| fact.label == label)
            .map(|fact| &fact.value)
    }

    fn tables(element: ElementRef<'_>) -> Vec<MobTable> {
        element
            .select(&selector("table"))
            .map(|table| {
                let region_label = table
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .find(|e| e.value().attr("role") == Some("region"))
                    .and_then(|e| e.value().attr("aria-label"))
                    .map(str::to_string);
                MobTable {
                    caption: first(table, "caption").map(text),
                    aria_label: table
                        .value()
                        .attr("aria-label")
                        .map(str::to_string)
                        .or(region_label),
                    headers: table.select(&selector("thead th")).map(value).collect(),
                    rows: table
                        .select(&selector("tr"))
                        .filter_map(|row| {
                            let cells: Vec<_> = row
                                .children()
                                .filter_map(ElementRef::wrap)
                                .filter(|e| matches!(e.value().name(), "td" | "th"))
                                .collect();
                            cells
                                .iter()
                                .any(|e| e.value().name() == "td")
                                .then(|| cells.into_iter().map(value).collect())
                        })
                        .collect(),
                }
            })
            .collect()
    }

    fn integer(input: &str) -> Option<u64> {
        input.trim().replace(',', "").parse().ok()
    }

    fn range(input: &str) -> Option<MobRange> {
        let (min, max) = input
            .trim()
            .split_once('–')
            .or_else(|| input.trim().split_once('-'))
            .unwrap_or((input, input));
        Some(MobRange {
            min: integer(min)?,
            max: integer(max)?,
        })
    }

    fn percent(input: &str) -> Option<f64> {
        let n = input.trim().strip_suffix('%')?.trim().parse::<f64>().ok()?;
        (n.is_finite() && (0.0..=100.0).contains(&n)).then_some(n)
    }

    fn map_count(input: &str) -> Option<u64> {
        // Restrict to a published map-location label, not any arbitrary integer.
        let prefix = input.split_once(" map location")?.0;
        integer(prefix.split_whitespace().last()?)
    }

    fn parse_drop_table(table: ElementRef<'_>) -> Vec<MobDrop> {
        let headers: Vec<_> = table.select(&selector("thead th")).map(text).collect();
        let column = |label: &str| headers.iter().position(|h| h == label);
        let Some(item_index) = column("Item") else {
            return Vec::new();
        };
        table
            .select(&selector("tbody tr"))
            .filter_map(|row| {
                let cells: Vec<_> = row
                    .children()
                    .filter_map(ElementRef::wrap)
                    .filter(|e| e.value().name() == "td")
                    .collect();
                let cell = |label| column(label).and_then(|i| cells.get(i)).copied();
                let item = value(*cells.get(item_index)?);
                let rolls = cell("Drop rolls")
                    .map(|cell| {
                        // Each innermost span is one roll, even for repeated identical rolls.
                        let leaves: Vec<_> = cell
                            .select(&selector("span"))
                            .filter(|e| first(*e, "span").is_none())
                            .collect();
                        let leaves = if leaves.is_empty() {
                            vec![cell]
                        } else {
                            leaves
                        };
                        leaves
                            .into_iter()
                            .map(|element| {
                                let raw = value(element);
                                let parts = raw.text.rsplit_once(' ');
                                MobDropRoll {
                                    source: parts.map(|(source, _)| source.to_string()),
                                    chance_percent: parts.and_then(|(_, chance)| percent(chance)),
                                    raw,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let chance = cell("Solo chance, at least one").and_then(|e| {
                    // The remainder contains the "Plan attempts" link text.
                    let t = text(e);
                    let end = t.find('%')?;
                    percent(&t[..=end])
                });
                let maximum_quantity =
                    cell("Maximum quantity").and_then(|e| integer(text(e).strip_prefix("Up to ")?));
                Some(MobDrop {
                    item,
                    rolls,
                    solo_chance_at_least_one_percent: chance,
                    maximum_quantity,
                    raw_cells: cells.into_iter().map(value).collect(),
                })
            })
            .collect()
    }

    fn parse_profile(details: ElementRef<'_>) -> MobDropProfile {
        let profile_facts = facts(details);
        let summary = first(details, "summary")
            .map(value)
            .unwrap_or_else(|| value(details));
        // Read the count's own span to avoid concatenating profile number and count.
        let map_location_count = first(details, "summary").and_then(|s| {
            s.select(&selector("span"))
                .find_map(|e| map_count(&text(e)))
                .or_else(|| map_count(&text(s)))
        });
        let solo_coins = fact(&profile_facts, "Solo coin result").and_then(|v| {
            let number = v
                .text
                .strip_suffix(" coins")
                .or_else(|| v.text.strip_suffix(" coin"))?;
            range(number)
        });
        MobDropProfile {
            summary,
            zone: fact(&profile_facts, "Zone").cloned(),
            map_location_count,
            solo_coins,
            drops: details
                .select(&selector("table"))
                .flat_map(parse_drop_table)
                .collect(),
            facts: profile_facts,
            tables: tables(details),
            raw: value(details),
        }
    }

    fn parse_mob(slug: &str, html: &str) -> Result<Mob> {
        let document = Html::parse_document(html);
        let main = document
            .select(&selector("main"))
            .next()
            .context("Missing mob main element")?;
        let name = first(main, "h1")
            .map(text)
            .filter(|s| !s.is_empty())
            .context("Missing mob name")?;
        let quick = first(main, "aside dl").context("Missing mob quick facts")?;
        let quick_facts = facts(quick);
        ensure!(
            fact(&quick_facts, "Level").is_some() && fact(&quick_facts, "Health").is_some(),
            "Missing mob Level/Health facts"
        );
        let get = |label| fact(&quick_facts, label);
        let combat = first(main, "#combat");
        let combat_facts = combat.map(facts).unwrap_or_default();
        let attack = get("Attack speed").map(|v| v.text.as_str());
        let attack_speed_ms = attack.and_then(|t| integer(t.split_once(" ms")?.0));
        let attacks = if attack == Some("Does not attack") {
            Some(false)
        } else {
            attack_speed_ms.map(|_| true)
        };
        let locations = main
            .select(&selector("#locations a[href^='/zones/']"))
            .map(|e| MobLocation {
                zone: MobLink {
                    text: first(e, "strong").map(text).unwrap_or_else(|| text(e)),
                    ..link(e)
                },
                map_location_count: first(e, "small").and_then(|e| map_count(&text(e))),
                raw: value(e),
            })
            .collect();
        let debuffs = combat
            .map(|combat| {
                combat
                    .select(&selector("table"))
                    .filter(|table| {
                        table
                            .select(&selector("th"))
                            .any(|h| text(h) == "Debuff skill")
                    })
                    .flat_map(|table| {
                        table
                            .select(&selector("tbody td"))
                            .map(value)
                            .collect::<Vec<_>>()
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Mob {
            slug: slug.to_string(),
            name,
            source_url: format!("{BASE_URL}/mobs/{slug}"),
            level: get("Level").and_then(|v| integer(&v.text)),
            health: get("Health").and_then(|v| integer(v.text.strip_suffix(" HP")?)),
            damage: get("Damage").and_then(|v| range(&v.text)),
            attack_speed_ms,
            attacks,
            faction: get("Faction").cloned(),
            faction_xp: get("Faction XP").and_then(|v| v.text.replace(',', "").parse().ok()),
            required_weapon: get("Required weapon").cloned(),
            aggressive: fact(&combat_facts, "Aggressive").and_then(|v| match v.text.as_str() {
                "Yes" => Some(true),
                "No" => Some(false),
                _ => None,
            }),
            debuff_skill_count: fact(&combat_facts, "Debuff skills").and_then(|v| integer(&v.text)),
            debuffs,
            locations,
            drop_profiles: main
                .select(&selector("#drops details"))
                .map(parse_profile)
                .collect(),
            raw: MobRawPage {
                facts: facts(main),
                sections: main
                    .select(&selector("section, aside"))
                    .map(|e| MobSection {
                        id: e.value().attr("id").map(str::to_string),
                        heading: first(e, "h2, h3").map(text),
                        facts: facts(e),
                        tables: tables(e),
                        content: value(e),
                    })
                    .collect(),
                tables: tables(main),
                links: value(main).links,
                source_html: html.to_string(),
            },
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // Reduced semantic markup/values from the live pages inspected 2026-09-15.
        // Shared boilerplate uses The False King's facts; other tests substitute
        // selected values, not complete snapshots. Calculator links are shortened.
        // Cosmetic classes are deliberately absent. The no-drop case is synthetic:
        // even the inspected Scarecrow and Iron Vein have global item rolls.
        fn page(name: &str, health: &str, damage: &str, speed: &str, extra: &str) -> String {
            format!(
                r#"<main><h1>{name}</h1><aside><h2>Quick facts</h2><dl>
            <div><dt>Level</dt><dd>130</dd></div>
            <div><dt>Health</dt><dd>{health}<!-- --> HP</dd></div>
            <div><dt>Damage</dt><dd>{damage}</dd></div>
            <div><dt>Attack speed</dt><dd>{speed}</dd></div>
            <div><dt>Faction</dt><dd><a href='/factions/kings-watch-18'>King's Watch</a></dd></div>
            <div><dt>Faction XP</dt><dd>1,500</dd></div>
            <div><dt>Required weapon</dt><dd>No enforced weapon type</dd></div>
            </dl></aside>{extra}</main>"#
            )
        }

        const ROLLS: &str = r#"<table><thead><tr><th>Item</th><th>Drop rolls</th>
        <th>Solo chance, at least one</th><th>Maximum quantity</th></tr></thead>
        <tbody><tr><td><a href='/items/kings-gold-3115'>King's Gold</a></td>
        <td><span><span>Spawn 100%</span><span>Spawn 100%</span><span>Spawn 100%</span></span></td>
        <td>100%<br><a href='/tools/drop-chance-calculator#chance=100&amp;sr=100&amp;sr=100&amp;sr=100'>Plan attempts</a></td>
        <td>Up to <!-- -->3</td></tr></tbody></table>"#;

        fn profile(zone: &str, coins: &str, body: &str) -> String {
            format!(
                r#"<details><summary><span>{zone} · Drop profile <!-- -->1</span><span>1<!-- --> map location</span></summary>
            <div><dl><div><dt>Zone</dt><dd>{zone}</dd></div>
            <div><dt>Solo coin result</dt><dd>{coins}</dd></div></dl>{body}</div></details>"#
            )
        }

        #[test]
        fn boss_repeated_rolls_and_lossless_roundtrip() {
            let extra = format!(
                "<section id='drops'><h2>Coins and drops</h2>{}</section>",
                profile("King's Keep", "125,000–174,999 coins", ROLLS)
            );
            let html = page(
                "The False King",
                "8,888,888",
                "65,000–90,000",
                "2,000 ms (2 sec)",
                &extra,
            );
            let mob = parse_mob("the-false-king-507", &html).unwrap();
            assert_eq!(mob.health, Some(8_888_888));
            assert_eq!(
                mob.damage,
                Some(MobRange {
                    min: 65_000,
                    max: 90_000
                })
            );
            assert_eq!(mob.attack_speed_ms, Some(2000));
            assert_eq!(mob.faction_xp, Some(1500));
            let profile = &mob.drop_profiles[0];
            assert_eq!(profile.map_location_count, Some(1));
            assert_eq!(
                profile.solo_coins,
                Some(MobRange {
                    min: 125_000,
                    max: 174_999
                })
            );
            assert_eq!(profile.drops[0].rolls.len(), 3);
            assert!(
                profile.drops[0]
                    .rolls
                    .iter()
                    .all(|r| r.chance_percent == Some(100.0))
            );
            assert_eq!(profile.drops[0].maximum_quantity, Some(3));
            assert_eq!(
                profile.drops[0].solo_chance_at_least_one_percent,
                Some(100.0)
            );
            assert!(
                profile.drops[0].raw_cells[2].links[0]
                    .href
                    .ends_with("&sr=100&sr=100&sr=100")
            );
            assert_eq!(mob.raw.source_html, html);
            assert_eq!(
                serde_json::from_str::<Mob>(&serde_json::to_string(&mob).unwrap()).unwrap(),
                mob
            );
        }

        #[test]
        fn multiple_profiles_locations_and_fractional_chances() {
            let rolls = ROLLS
                .replace("Spawn 100%", "Zone 0.8%")
                .replace("100%<br>", "2.3808%<br>");
            let extra = format!(
                r#"<section id='locations'><a href='/zones/noob-island-1' aria-label='Noob Island. 11 map locations'>
            <strong>Noob Island</strong><small>11 map locations</small></a></section>
            <section id='drops'>{}{}</section>"#,
                profile("Noob Island", "1 coin", &rolls),
                profile("Hidden Reef", "0 coins", ROLLS)
            );
            let mob = parse_mob(
                "soft-shelled-crab-1",
                &page(
                    "Soft Shelled Crab",
                    "20",
                    "1–1",
                    "2,200 ms (2.2 sec)",
                    &extra,
                ),
            )
            .unwrap();
            assert_eq!(mob.locations[0].map_location_count, Some(11));
            assert_eq!(mob.locations[0].zone.text, "Noob Island");
            assert_eq!(mob.drop_profiles.len(), 2);
            assert_eq!(
                mob.drop_profiles[0].solo_coins,
                Some(MobRange { min: 1, max: 1 })
            );
            assert_eq!(
                mob.drop_profiles[1].solo_coins,
                Some(MobRange { min: 0, max: 0 })
            );
            assert_eq!(
                mob.drop_profiles[0].drops[0].rolls[0].chance_percent,
                Some(0.8)
            );
        }

        #[test]
        fn non_attacking_no_drops_and_unknown_facts_survive() {
            let extra = format!(
                "<section id='drops'>{}</section><section id='future'><h2>Future traits</h2><dl><dt>Armor</dt><dd>Unpublished</dd></dl></section>",
                profile("Test zone", "0 coins", "<p>No item drops.</p>")
            );
            let mob = parse_mob(
                "iron-vein-46",
                &page("Iron Vein", "10", "0–0", "Does not attack", &extra),
            )
            .unwrap();
            assert_eq!(mob.attacks, Some(false));
            assert_eq!(mob.attack_speed_ms, None);
            assert_eq!(mob.damage, Some(MobRange { min: 0, max: 0 }));
            assert!(mob.drop_profiles[0].drops.is_empty());
            assert!(mob.drop_profiles[0].raw.text.contains("No item drops."));
            assert_eq!(fact(&mob.raw.facts, "Armor").unwrap().text, "Unpublished");
            let absent = parse_mob(
                "test-1",
                &page("Test", "unknown HP", "unknown", "Unknown", ""),
            )
            .unwrap();
            assert_eq!(absent.health, None);
            assert_eq!(absent.attacks, None);
            assert!(absent.drop_profiles.is_empty());
            assert_ne!(absent.raw.source_html, mob.raw.source_html);
        }

        #[test]
        fn combat_debuffs_including_unlisted() {
            let extra = r#"<section id='combat'><h2>Combat behavior</h2><div>
            <div><span>Aggressive</span><strong>No</strong></div>
            <div><span>Debuff skills</span><strong>1</strong></div></div>
            <table><thead><tr><th>Debuff skill</th></tr></thead><tbody><tr><td>Unlisted debuff</td></tr></tbody></table></section>"#;
            let mob = parse_mob(
                "tiamats-shadow-478",
                &page(
                    "Tiamat's Shadow",
                    "3,000,000",
                    "35,000–46,000",
                    "1,800 ms (1.8 sec)",
                    extra,
                ),
            )
            .unwrap();
            assert_eq!(mob.aggressive, Some(false));
            assert_eq!(mob.debuff_skill_count, Some(1));
            assert_eq!(mob.debuffs[0].text, "Unlisted debuff");
            assert_eq!(mob.raw.tables[0].headers[0].text, "Debuff skill");
        }

        #[test]
        fn rejects_non_detail_pages_and_unsafe_slugs() {
            for html in [
                "",
                "<main><h1>Login</h1></main>",
                "<main><h1>Mobs</h1><aside><dl></dl></aside></main>",
            ] {
                assert!(parse_mob("test-1", html).is_err());
            }
            for slug in ["", "../items/test-1", "test-1?x=1", "test-1#fragment"] {
                assert!(fetch_mob(slug).is_err());
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{fetch_mob, fetch_mob_slugs};
