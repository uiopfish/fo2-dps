use anyhow::{Context, Result};
use scraper::{Html, Selector};

use crate::db::{
    Item, ItemRequirements, ItemSet, ItemSetBonus, ItemSetBonusEffect, ItemSetPiece, ItemStats,
    Skill, SkillAttributeRequirement, SkillCooldown, SkillDuration, SkillEffect, SkillQuickFact,
};

const BASE_URL: &str = "https://db.fantasyonline2.com";

const MAX_REQUEST_ATTEMPTS: u32 = 3;
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(750);
const INDEX_REQUEST_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

fn transient_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
}

fn transient_error(error: &reqwest::Error) -> bool {
    if let Some(status) = error.status() {
        return transient_status(status);
    }
    error.is_timeout() || error.is_connect() || error.is_body()
}

pub(crate) fn fetch_page(url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("fo2-dps/0.1")
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    for attempt in 1..=MAX_REQUEST_ATTEMPTS {
        match client
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
        {
            Ok(body) => return Ok(body),
            Err(error) if attempt == MAX_REQUEST_ATTEMPTS || !transient_error(&error) => {
                return Err(error)
                    .with_context(|| format!("Request failed after {attempt} attempt(s): {url}"));
            }
            Err(error) => {
                eprintln!(
                    "Request attempt {attempt}/{MAX_REQUEST_ATTEMPTS} failed for {url}: {error}. Retrying..."
                );
                std::thread::sleep(RETRY_DELAY * (1 << (attempt - 1)));
            }
        }
    }

    unreachable!("Request loop always returns on success or final failure")
}

fn clean_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn fetch_item(slug: &str) -> Result<Item> {
    fetch_item_with_debug(slug, true)
}

pub fn fetch_item_for_bulk(slug: &str) -> Result<Item> {
    fetch_item_with_debug(slug, false)
}

fn fetch_item_with_debug(slug: &str, save_debug_html: bool) -> Result<Item> {
    let url = format!("{BASE_URL}/items/{slug}");

    println!("Fetching {url}");

    let html = fetch_page(&url)?;
    if save_debug_html {
        std::fs::write("data/debug-item.html", &html)?;
    }

    parse_item_html(slug, &html)
}

fn parse_item_html(slug: &str, html: &str) -> Result<Item> {
    let document = Html::parse_document(html);
    let tooltip = Selector::parse("[data-item-tooltip=\"true\"]").unwrap();
    let tooltip = document
        .select(&tooltip)
        .next()
        .context("Could not find item tooltip")?;

    let h1 = Selector::parse("h1").unwrap();
    let name = tooltip
        .select(&h1)
        .next()
        .map(|element| clean_text(&element.text().collect::<String>()))
        .context("Could not find item name")?;

    let description_selector = Selector::parse("[class*='item-tooltip_description__']").unwrap();
    let description = tooltip
        .select(&description_selector)
        .next()
        .map(|element| clean_text(&element.text().collect::<String>()))
        .filter(|description| !description.is_empty());
    let image_selector = Selector::parse("meta[property='og:image']").unwrap();
    let image_url = document
        .select(&image_selector)
        .next()
        .and_then(|element| element.value().attr("content"))
        .map(str::to_string)
        .filter(|url| !url.is_empty());

    let line_selector = Selector::parse(".item-tooltip_line__9hb2F").unwrap();
    let lines: Vec<String> = tooltip
        .select(&line_selector)
        .map(|element| clean_text(&element.text().collect::<String>()))
        .collect();

    let item_type = lines.first().cloned().context("Could not find item type")?;
    let damage = lines.iter().find_map(|line| extract_damage(line));
    let attack_speed = lines.iter().find_map(|line| extract_attack_speed(line));

    let mut stats = ItemStats::default();
    let mut requirements = ItemRequirements::default();
    let mut market_price = None;
    let mut recently_sold_price = None;
    let mut shop_price = None;
    let mut sellable_to_shops = false;
    let mut rebirth = false;
    let mut ascension = false;

    let mut unparsed_tooltip_lines = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let parsed = index == 0
            || extract_damage(line).is_some()
            || extract_attack_speed(line).is_some()
            || apply_item_typed_line(&mut stats, &mut requirements, &mut ascension, line)
            || extract_price(line, "Market median price:").is_some_and(|price| {
                market_price = Some(price);
                true
            })
            || extract_price(line, "Recently sold for:").is_some_and(|price| {
                recently_sold_price = Some(price);
                true
            })
            || extract_price(line, "Sells to shops for:").is_some_and(|price| {
                shop_price = Some(price);
                sellable_to_shops = true;
                true
            })
            || line == "NOT SELLABLE TO SHOPS"
            || line == "Rebirth"
            || line.ends_with("Set Bonus")
            || line.starts_with("Bonus at:");

        if line == "NOT SELLABLE TO SHOPS" {
            sellable_to_shops = false;
        }
        if line == "Rebirth" {
            rebirth = true;
        }
        if !parsed {
            unparsed_tooltip_lines.push(line.clone());
        }
    }

    let implant_slot = description
        .as_deref()
        .and_then(crate::db::ImplantSlot::from_description);

    Ok(Item {
        name,
        slug: slug.to_string(),
        item_type,
        description,
        image_url,
        implant_slot,
        damage_min: damage.map(|(min, _)| min),
        damage_max: damage.map(|(_, max)| max),
        attack_speed,
        stats,
        requirements,
        market_price,
        recently_sold_price,
        shop_price,
        sellable_to_shops,
        rebirth,
        ascension,
        unparsed_tooltip_lines,
    })
}

pub fn fetch_item_set(slug: &str) -> Result<ItemSet> {
    let url = format!("{BASE_URL}/item-sets/{slug}");

    println!("Fetching {url}");

    let html = fetch_page(&url)?;
    std::fs::write("data/debug-item-set.html", &html)?;

    let document = Html::parse_document(&html);
    let h1 = Selector::parse("h1").unwrap();
    let name = document
        .select(&h1)
        .next()
        .map(|element| clean_text(&element.text().collect::<String>()))
        .context("Could not find item-set name")?;

    let rows = Selector::parse("tbody tr").unwrap();
    let cells = Selector::parse("td").unwrap();
    let item_link = Selector::parse("a[href^=\"/items/\"]").unwrap();
    let quick_fact = Selector::parse("aside[aria-label=\"Item-set quick facts\"] div").unwrap();
    let fact_label = Selector::parse("dt").unwrap();
    let fact_value = Selector::parse("dd").unwrap();

    let quick_facts: Vec<(String, String)> = document
        .select(&quick_fact)
        .filter_map(|fact| {
            let label = fact
                .select(&fact_label)
                .next()
                .map(|element| clean_text(&element.text().collect::<String>()))?;
            let value = fact
                .select(&fact_value)
                .next()
                .map(|element| clean_text(&element.text().collect::<String>()))?;

            Some((label, value))
        })
        .collect();
    let fact = |label: &str| {
        quick_facts
            .iter()
            .find_map(|(fact_label, value)| (fact_label == label).then_some(value.clone()))
            .with_context(|| format!("Could not find item-set quick fact: {label}"))
    };

    let bonus_rule = fact("Bonus rule")?;
    let resource_tool_rule = fact("Resource-tool rule")?;

    let pieces_section = Selector::parse("section#pieces").unwrap();
    let pieces = document
        .select(&pieces_section)
        .next()
        .context("Could not find item-set pieces section")?
        .select(&rows)
        .map(|row| {
            let cells: Vec<String> = row
                .select(&cells)
                .map(|cell| clean_text(&cell.text().collect::<String>()))
                .collect();

            let link = row
                .select(&item_link)
                .next()
                .context("Could not find item-set piece link")?;
            let slug = link
                .value()
                .attr("href")
                .and_then(|href| href.strip_prefix("/items/"))
                .context("Could not parse item-set piece slug")?;

            let [name, item_type, level_requirement] = cells.as_slice() else {
                anyhow::bail!("Unexpected item-set piece row: {cells:?}");
            };

            Ok(ItemSetPiece {
                name: name.clone(),
                slug: slug.to_string(),
                item_type: item_type.clone(),
                level_requirement: parse_u32(level_requirement),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let bonuses_section = Selector::parse("section#bonuses").unwrap();
    let bonuses = document
        .select(&bonuses_section)
        .next()
        .context("Could not find item-set bonuses section")?
        .select(&rows)
        .map(|row| {
            let cells: Vec<String> = row
                .select(&cells)
                .map(|cell| clean_text(&cell.text().collect::<String>()))
                .collect();

            let [required_pieces, effects] = cells.as_slice() else {
                anyhow::bail!("Unexpected item-set bonus row: {cells:?}");
            };

            let required_pieces = parse_u32(required_pieces)
                .with_context(|| format!("Invalid item-set piece threshold: {required_pieces}"))?;

            Ok(ItemSetBonus {
                required_pieces,
                effects: effects
                    .split('·')
                    .map(|effect| {
                        let (stat, value) = effect
                            .trim()
                            .rsplit_once(' ')
                            .context("Could not parse item-set bonus effect")?;

                        Ok(ItemSetBonusEffect {
                            stat: stat.to_string(),
                            value: value.to_string(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ItemSet {
        name,
        slug: slug.to_string(),
        bonus_rule,
        resource_tool_rule,
        pieces,
        bonuses,
    })
}

pub fn fetch_skill(slug: &str) -> Result<Skill> {
    fetch_skill_with_debug(slug, true)
}

pub fn fetch_skill_for_bulk(slug: &str) -> Result<Skill> {
    fetch_skill_with_debug(slug, false)
}

fn fetch_skill_with_debug(slug: &str, save_debug_html: bool) -> Result<Skill> {
    let url = format!("{BASE_URL}/skills/{slug}");

    println!("Fetching {url}");

    let html = fetch_page(&url)?;
    if save_debug_html {
        std::fs::write("data/debug-skill.html", &html)?;
    }
    let document = Html::parse_document(&html);

    let h1 = Selector::parse("h1").unwrap();
    let name = document
        .select(&h1)
        .next()
        .map(|element| clean_text(&element.text().collect::<String>()))
        .context("Could not find skill name")?;

    let quick_fact = Selector::parse("aside[aria-label$=\"quick facts\"] div").unwrap();
    let fact_label = Selector::parse("dt").unwrap();
    let fact_value = Selector::parse("dd").unwrap();
    let quick_facts: Vec<(String, String)> = document
        .select(&quick_fact)
        .filter_map(|fact| {
            let label = fact
                .select(&fact_label)
                .next()
                .map(|element| clean_text(&element.text().collect::<String>()))?;
            let value = fact
                .select(&fact_value)
                .next()
                .map(|element| clean_text(&element.text().collect::<String>()))?;

            Some((label, value))
        })
        .collect();
    let fact = |label: &str| {
        quick_facts
            .iter()
            .find_map(|(fact_label, value)| (fact_label == label).then_some(value.as_str()))
            .with_context(|| format!("Could not find skill quick fact: {label}"))
    };

    let rank = Some(parse_u32(fact("Rank")?).context("Invalid skill rank")?);
    let level_requirement = parse_u32(fact("Level requirement")?);
    let cast_time = parse_seconds(fact("Base cast time")?);
    let duration = Some(parse_duration(fact("Duration")?)?);
    let cooldown = Some(parse_cooldown(fact("Cooldown")?)?);
    let energy_cost = parse_u32(fact("Energy cost")?);

    let rows = Selector::parse("tbody tr").unwrap();
    let cells = Selector::parse("td").unwrap();
    let requirements_section = Selector::parse("section#requirements").unwrap();
    let requirement_attribute = Selector::parse("span").unwrap();
    let requirement_amount = Selector::parse("strong").unwrap();
    let requirements_section = document
        .select(&requirements_section)
        .next()
        .context("Could not find skill requirements section")?;
    let attributes: Vec<String> = requirements_section
        .select(&requirement_attribute)
        .map(|element| clean_text(&element.text().collect::<String>()))
        .collect();
    let amounts: Vec<String> = requirements_section
        .select(&requirement_amount)
        .map(|element| clean_text(&element.text().collect::<String>()))
        .collect();

    if attributes.len() != amounts.len() {
        anyhow::bail!("Mismatched skill requirement attributes and values");
    }

    let attribute_requirements = attributes
        .into_iter()
        .zip(amounts)
        .map(|(attribute, amount)| {
            Ok(SkillAttributeRequirement {
                attribute,
                amount: parse_u32(&amount),
                value: amount,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let effects_section = Selector::parse("section#effects").unwrap();
    let effects = document
        .select(&effects_section)
        .next()
        .context("Could not find skill effects section")?
        .select(&rows)
        .map(|row| {
            let cells: Vec<String> = row
                .select(&cells)
                .map(|cell| clean_text(&cell.text().collect::<String>()))
                .collect();
            let [effect_type, details] = cells.as_slice() else {
                anyhow::bail!("Unexpected skill effect row: {cells:?}");
            };

            Ok(SkillEffect {
                effect_type: effect_type.clone(),
                details: details.clone(),
                components: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let mut skill = Skill {
        name,
        slug: slug.to_string(),
        rank,
        level_requirement,
        attribute_requirements,
        quick_facts: quick_facts
            .into_iter()
            .map(|(label, value)| SkillQuickFact { label, value })
            .collect(),
        cast_time,
        duration,
        cooldown,
        energy_cost,
        effects,
    };
    crate::effects::normalize_skill_effects(&mut skill);
    Ok(skill)
}

fn parse_u32(text: &str) -> Option<u32> {
    text.replace(',', "").trim().parse::<u32>().ok()
}

fn parse_u64(text: &str) -> Option<u64> {
    text.replace(',', "").trim().parse::<u64>().ok()
}

fn parse_seconds(text: &str) -> Option<f32> {
    if text == "Instant" {
        return Some(0.0);
    }
    let (amount, unit) = text.split_once(' ')?;
    let amount = amount.parse::<f32>().ok()?;

    match unit {
        "ms" => Some(amount / 1000.0),
        "second" | "seconds" => Some(amount),
        "minute" | "minutes" => Some(amount * 60.0),
        "hour" | "hours" => Some(amount * 3600.0),
        _ => None,
    }
}

fn parse_duration(text: &str) -> Result<SkillDuration> {
    if text == "No timed duration" {
        return Ok(SkillDuration::NoTimedDuration);
    }

    parse_seconds(text)
        .map(SkillDuration::Seconds)
        .with_context(|| format!("Invalid skill duration: {text}"))
}

fn parse_cooldown(text: &str) -> Result<SkillCooldown> {
    if text == "None" {
        return Ok(SkillCooldown::None);
    }

    parse_seconds(text)
        .map(SkillCooldown::Seconds)
        .with_context(|| format!("Invalid skill cooldown: {text}"))
}

fn extract_stat_bonus(text: &str) -> Option<(&str, i32)> {
    let (amount, stat) = text.split_once(' ')?;
    if !amount.starts_with('+') && !amount.starts_with('-') {
        return None;
    }

    Some((stat, parse_i32(amount)?))
}

/// Reparse preserved item stat, requirement, and Ascension lines without fetching data.
/// Returns the number of consumed lines. Recognized values replace existing values;
/// unknown or invalid lines retain their original text and order. Repeated calls are
/// idempotent. The caller is responsible for loading and saving its item dataset.
pub fn normalize_item_from_unparsed_lines(item: &mut Item) -> usize {
    item.implant_slot = item
        .description
        .as_deref()
        .and_then(crate::db::ImplantSlot::from_description);
    let before = item.unparsed_tooltip_lines.len();
    item.unparsed_tooltip_lines.retain(|line| {
        !apply_item_typed_line(
            &mut item.stats,
            &mut item.requirements,
            &mut item.ascension,
            line,
        )
    });
    before - item.unparsed_tooltip_lines.len()
}

fn apply_item_typed_line(
    stats: &mut ItemStats,
    requirements: &mut ItemRequirements,
    ascension: &mut bool,
    line: &str,
) -> bool {
    if line == "Ascension" {
        *ascension = true;
        return true;
    }
    extract_stat_bonus(line).is_some_and(|(stat, value)| apply_item_stat(stats, stat, value))
        || apply_item_requirement(requirements, line)
}

fn apply_item_stat(stats: &mut ItemStats, stat: &str, value: i32) -> bool {
    match stat {
        "Armor" => stats.armor = Some(value),
        "Stamina" => stats.stamina = Some(value),
        "Strength" => stats.strength = Some(value),
        "Agility" => stats.agility = Some(value),
        "Intellect" => stats.intellect = Some(value),
        "Attack power" | "Attack Power" => stats.attack_power = Some(value),
        "Crit" => stats.crit = Some(value),
        "Damage" => stats.damage = Some(value),
        "Cast Time Reduction" => stats.cast_time_reduction = Some(value),
        "Max Health" => stats.max_health = Some(value),
        "Max Energy" => stats.max_energy = Some(value),
        "Health Regen" => stats.health_regen = Some(value),
        "Energy Regen" => stats.energy_regen = Some(value),
        _ => return false,
    }

    true
}

fn apply_item_requirement(requirements: &mut ItemRequirements, text: &str) -> bool {
    for (prefix, target) in [
        ("Faction Notoriety ", &mut requirements.faction_notoriety),
        ("Guild Level ", &mut requirements.guild_level),
    ] {
        if let Some(amount) = text.strip_prefix(prefix) {
            let Some(amount) = parse_u32(amount) else {
                return false;
            };
            *target = Some(amount);
            return true;
        }
    }

    let Some((attribute, amount)) = text.split_once(' ') else {
        return false;
    };
    let Some(amount) = parse_u32(amount) else {
        return false;
    };

    match attribute {
        "Level" => requirements.level = Some(amount),
        "Stamina" => requirements.stamina = Some(amount),
        "Strength" => requirements.strength = Some(amount),
        "Agility" => requirements.agility = Some(amount),
        "Intellect" => requirements.intellect = Some(amount),

        _ => return false,
    }

    true
}

fn parse_i32(text: &str) -> Option<i32> {
    text.replace(',', "").trim().parse::<i32>().ok()
}

fn extract_price(text: &str, label: &str) -> Option<u64> {
    parse_u64(text.strip_prefix(label)?.trim())
}

fn extract_damage(text: &str) -> Option<(u32, u32)> {
    let damage = text.strip_suffix("Damage")?.trim();

    let damage = damage.strip_prefix('(')?.strip_suffix(')')?;

    let (min, max) = damage.split_once('-')?;

    let min = parse_u32(min.trim())?;
    let max = parse_u32(max.trim())?;

    Some((min, max))
}

fn extract_attack_speed(text: &str) -> Option<f32> {
    let value = text.strip_suffix("Attack Speed")?.trim();

    value.parse::<f32>().ok()
}

fn valid_detail_slug(slug: &str) -> bool {
    let Some((name, id)) = slug.rsplit_once('-') else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !id.is_empty()
        && id.bytes().all(|byte| byte.is_ascii_digit())
}

fn discover_slugs(
    collection: &str,
    mut fetch: impl FnMut(&str) -> Result<String>,
) -> Result<Vec<String>> {
    use std::collections::{BTreeSet, HashSet, VecDeque};

    let root = format!("{BASE_URL}/{collection}");
    let prefix = format!("/{collection}/");
    let mut pending = VecDeque::from([root.clone()]);
    let mut visited = HashSet::from([root]);
    let mut slugs = BTreeSet::new();
    let links = Selector::parse("a[href]").unwrap();

    while let Some(url) = pending.pop_front() {
        let html = fetch(&url).with_context(|| format!("Could not fetch index: {url}"))?;
        let document = Html::parse_document(&html);
        let base = reqwest::Url::parse(&url)?;
        let mut page_slugs = 0;
        for link in document.select(&links) {
            let Some(href) = link.value().attr("href") else {
                continue;
            };
            let Ok(mut target) = base.join(href) else {
                continue;
            };
            if target.origin() != base.origin() {
                continue;
            }
            target.set_query(None);
            target.set_fragment(None);
            let Some(suffix) = target.path().strip_prefix(&prefix) else {
                continue;
            };
            if let Some(page) = suffix.strip_prefix("page/") {
                if page.is_empty() || !page.bytes().all(|byte| byte.is_ascii_digit()) {
                    continue;
                }
                let Ok(page) = page.parse::<u32>() else {
                    continue;
                };
                if page == 0 {
                    continue;
                }
                // The root index and page/1 represent the same page.
                let next = if page == 1 {
                    format!("{BASE_URL}/{collection}")
                } else {
                    target.to_string()
                };
                if visited.insert(next.clone()) {
                    pending.push_back(next);
                }
            } else if valid_detail_slug(suffix) {
                page_slugs += 1;
                slugs.insert(suffix.to_string());
            }
        }
        anyhow::ensure!(page_slugs > 0, "Empty {collection} index: {url}");
    }

    Ok(slugs.into_iter().collect())
}

pub(crate) fn fetch_index_slugs(collection: &str) -> Result<Vec<String>> {
    let mut first = true;
    discover_slugs(collection, |url| {
        if !first {
            std::thread::sleep(INDEX_REQUEST_DELAY);
        }
        first = false;
        println!("Fetching {url}");
        fetch_page(url)
    })
}

pub fn fetch_item_slugs() -> Result<Vec<String>> {
    fetch_index_slugs("items")
}

pub fn fetch_skill_slugs() -> Result<Vec<String>> {
    fetch_index_slugs("skills")
}

pub fn fetch_item_set_slugs() -> Result<Vec<String>> {
    let url = format!("{BASE_URL}/item-sets");

    println!("Fetching {url}");

    let html = fetch_page(&url)?;
    let document = Html::parse_document(&html);
    let links = Selector::parse("a[href^=\"/item-sets/\"]").unwrap();
    let mut slugs = Vec::new();

    for link in document.select(&links) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Some(slug) = href.strip_prefix("/item-sets/") else {
            continue;
        };

        if !slugs.iter().any(|existing| existing == slug) {
            slugs.push(slug.to_string());
        }
    }

    Ok(slugs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_follows_links_on_every_page_and_deduplicates() {
        for collection in ["items", "skills"] {
            let root = format!("{BASE_URL}/{collection}");
            let mut requests = Vec::new();
            let slugs = discover_slugs(collection, |url| {
                requests.push(url.to_string());
                let links = if url == root {
                    format!(
                        "<a href='/{collection}/first-1'>one</a>
                                                 <a href='/{collection}/valid--double-3'>double hyphen</a>
                         <a href='/{collection}/first-1#tooltip'>duplicate</a>
                         <a href='/{collection}/page/3'>next</a>
                         <a href='/{collection}/page/3'>duplicate page</a>
                         <a href='/{collection}/page/1'>first</a>
                         <a href='/{collection}/page/0'>zero</a>
                         <a href='/{collection}/page/no'>bad page</a>
                         <a href='/{collection}/page/2/extra'>nested page</a>
                         <a href='/{collection}/category'>category</a>
                         <a href='/{collection}/nested/item-5'>nested</a>
                         <a href='/{collection}/bad-id'>bad id</a>
                         <a href='/{collection}/-5'>empty name</a>
                         <a href='/{collection}/bad%2Fname-5'>encoded slash</a>
                         <a href='/other/wrong-5'>wrong collection</a>
                         <a href='https://example.com/{collection}/wrong-5'>external</a>"
                    )
                } else if url == format!("{root}/page/3") {
                    format!(
                        "<a href='{BASE_URL}/{collection}/second-2?view=all'>two</a>
                         <a href='/{collection}/page/7'>next</a>
                         <a href='/{collection}/page/1'>back</a>"
                    )
                } else if url == format!("{root}/page/7") {
                    format!(
                        "<a href='/{collection}/first-1'>duplicate</a>
                         <a href='../page/3'>cycle</a>"
                    )
                } else {
                    panic!("Unexpected index request: {url}");
                };
                Ok(links)
            })
            .unwrap();
            assert_eq!(slugs, ["first-1", "second-2", "valid--double-3"]);
            assert_eq!(
                requests,
                [
                    root.clone(),
                    format!("{root}/page/3"),
                    format!("{root}/page/7")
                ]
            );
        }
    }

    #[test]
    fn discovery_rejects_empty_indexes_and_propagates_errors() {
        for collection in ["items", "skills"] {
            assert!(discover_slugs(collection, |_| Ok("<html>Login</html>".into())).is_err());
            assert!(discover_slugs(collection, |_| anyhow::bail!("offline failure")).is_err());
            let mut count = 0;
            assert!(discover_slugs(collection, |_| {
                count += 1;
                Ok(if count == 1 {
                    format!("<a href='/{collection}/valid-1'>item</a><a href='/{collection}/page/2'>next</a>")
                } else {
                    String::new()
                })
            }).is_err());
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn seconds_units_and_invalid_values() {
        for (text, expected) in [
            ("Instant", 0.0),
            ("250 ms", 0.25),
            ("1 second", 1.0),
            ("1.5 seconds", 1.5),
            ("1 minute", 60.0),
            ("2 minutes", 120.0),
            ("1 hour", 3600.0),
            ("2 hours", 7200.0),
        ] {
            assert_eq!(parse_seconds(text), Some(expected), "{text}");
        }
        for text in ["", "None", "abc seconds", "1 day", "2", "1 seconds extra"] {
            assert_eq!(parse_seconds(text), None, "{text}");
        }
    }

    #[test]
    fn signed_comma_stats_and_stamina_requirements_are_distinct() {
        let mut stats = ItemStats::default();
        let mut requirements = ItemRequirements::default();
        for line in ["-1,234 Armor", "+2,345 Attack Power", "+12 Stamina"] {
            let (stat, value) = extract_stat_bonus(line).unwrap();
            assert!(apply_item_stat(&mut stats, stat, value));
            assert!(!apply_item_requirement(&mut requirements, line));
        }
        assert_eq!(stats.armor, Some(-1234));
        assert_eq!(stats.attack_power, Some(2345));
        assert_eq!(stats.stamina, Some(12));
        assert!(extract_stat_bonus("Stamina 1,200").is_none());
        assert!(apply_item_requirement(&mut requirements, "Stamina 1,200"));
        assert_eq!(requirements.stamina, Some(1200));
        assert_eq!(stats.stamina, Some(12));
        assert!(apply_item_stat(&mut stats, "Attack power", -5));
        assert_eq!(stats.attack_power, Some(-5));
        assert!(!apply_item_stat(&mut stats, "Unknown", 1));
        assert!(extract_stat_bonus("+bad Armor").is_none());
    }

    #[test]
    fn item_parser_preserves_description_and_artwork_outside_stat_lines() {
        let html = r#"
            <html><head>
              <meta property="og:image" content="https://art.fantasyonline2.com/item.png">
            </head><body>
              <div data-item-tooltip="true">
                <div><h1>Sacred Gauntlet Implant</h1></div>
                <span class="item-tooltip_description__hash">Implant for your Left Arm.</span>
                <span><span class="item-tooltip_line__9hb2F">Implant</span>
                <span class="item-tooltip_line__9hb2F">+7 Crit</span></span>
              </div>
            </body></html>
        "#;
        let item = parse_item_html("sacred-gauntlet-implant-1492", html).unwrap();
        assert_eq!(
            item.description.as_deref(),
            Some("Implant for your Left Arm.")
        );
        assert_eq!(
            item.image_url.as_deref(),
            Some("https://art.fantasyonline2.com/item.png")
        );
        assert_eq!(item.implant_slot, Some(crate::db::ImplantSlot::LeftArm));
        assert_eq!(item.stats.crit, Some(7));
    }

    #[test]
    fn item_normalization_is_backward_compatible_and_idempotent() {
        let mut item: Item = serde_json::from_value(serde_json::json!({
            "name": "Legacy item", "slug": "legacy", "item_type": "Implant",
            "damage_min": 12, "damage_max": 34, "attack_speed": 1.5,
            "stats": {"crit": 7},
            "requirements": {"level": 70, "faction_notoriety": 20000},
            "market_price": 88000, "recently_sold_price": null, "shop_price": 50000,
            "sellable_to_shops": true, "rebirth": false,
            "unparsed_tooltip_lines": [
                "+100 Damage", "-15 Cast Time Reduction", "+1,400 Max Health",
                "-1,200 Max Energy", "+100 Health Regen", "-35 Energy Regen",
                "Ascension", "Guild Level 800",
                "Fishing Power: +10", "Fishing Region:", "Freshwater",
                "+bad Damage", "+2147483648 Damage", "Guild Level -1",
                "Faction Notoriety invalid", "Ascension 2"
            ]
        }))
        .unwrap();
        assert!(!item.ascension);
        assert_eq!(item.stats.damage, None);
        assert_eq!(item.requirements.guild_level, None);
        assert_eq!(normalize_item_from_unparsed_lines(&mut item), 8);
        assert_eq!(item.stats.damage, Some(100));
        assert_eq!(item.stats.cast_time_reduction, Some(-15));
        assert_eq!(item.stats.max_health, Some(1400));
        assert_eq!(item.stats.max_energy, Some(-1200));
        assert_eq!(item.stats.health_regen, Some(100));
        assert_eq!(item.stats.energy_regen, Some(-35));
        assert_eq!(item.stats.crit, Some(7));
        assert!(item.ascension);
        assert!(!item.rebirth);
        assert_eq!(item.requirements.guild_level, Some(800));
        assert_eq!(item.requirements.level, Some(70));
        assert_eq!(item.requirements.faction_notoriety, Some(20000));
        assert_eq!((item.damage_min, item.damage_max), (Some(12), Some(34)));
        assert_eq!(item.market_price, Some(88000));
        assert_eq!(
            item.unparsed_tooltip_lines,
            [
                "Fishing Power: +10",
                "Fishing Region:",
                "Freshwater",
                "+bad Damage",
                "+2147483648 Damage",
                "Guild Level -1",
                "Faction Notoriety invalid",
                "Ascension 2"
            ]
        );
        let normalized = serde_json::to_value(&item).unwrap();
        assert_eq!(normalize_item_from_unparsed_lines(&mut item), 0);
        assert_eq!(serde_json::to_value(&item).unwrap(), normalized);
        let restored: Item = serde_json::from_value(normalized.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), normalized);
    }

    #[test]
    fn item_combat_stats_accept_both_signs_and_guild_requirement_is_distinct() {
        for stat in [
            "Damage",
            "Cast Time Reduction",
            "Max Health",
            "Max Energy",
            "Health Regen",
            "Energy Regen",
        ] {
            for amount in ["+1,234", "-1,234"] {
                let mut stats = ItemStats::default();
                let mut requirements = ItemRequirements::default();
                let mut ascension = false;
                let line = format!("{amount} {stat}");
                assert!(apply_item_typed_line(
                    &mut stats,
                    &mut requirements,
                    &mut ascension,
                    &line
                ));
                let value = match stat {
                    "Damage" => stats.damage,
                    "Cast Time Reduction" => stats.cast_time_reduction,
                    "Max Health" => stats.max_health,
                    "Max Energy" => stats.max_energy,
                    "Health Regen" => stats.health_regen,
                    _ => stats.energy_regen,
                };
                assert_eq!(value, parse_i32(amount));
                assert!(!apply_item_requirement(&mut requirements, &line));
            }
        }
        let mut requirements = ItemRequirements::default();
        assert!(apply_item_requirement(&mut requirements, "Level 104"));
        assert!(apply_item_requirement(
            &mut requirements,
            "Guild Level 1,200"
        ));
        assert!(!apply_item_requirement(
            &mut requirements,
            "Guild Level invalid"
        ));
        assert_eq!(requirements.level, Some(104));
        assert_eq!(requirements.guild_level, Some(1200));
    }

    #[test]
    fn damage_accepts_commas() {
        assert_eq!(extract_damage("(1,200 - 3,456) Damage"), Some((1200, 3456)));
        assert_eq!(extract_damage("(12-34) Damage"), Some((12, 34)));
        for text in ["12-34 Damage", "(bad-34) Damage", "(12-34) Armor"] {
            assert_eq!(extract_damage(text), None);
        }
    }

    #[test]
    fn duration_and_cooldown_preserve_special_values() {
        assert!(matches!(
            parse_duration("No timed duration").unwrap(),
            SkillDuration::NoTimedDuration
        ));
        assert!(
            matches!(parse_duration("2 minutes").unwrap(), SkillDuration::Seconds(s) if s == 120.0)
        );
        assert!(matches!(
            parse_cooldown("None").unwrap(),
            SkillCooldown::None
        ));
        assert!(
            matches!(parse_cooldown("250 ms").unwrap(), SkillCooldown::Seconds(s) if s == 0.25)
        );
        assert!(parse_duration("None").is_err());
        assert!(parse_cooldown("No timed duration").is_err());
        assert!(parse_duration("invalid").is_err());
        assert!(parse_cooldown("invalid").is_err());
    }

    #[test]
    fn only_transient_statuses_are_retried() {
        for code in [408, 429, 500, 502, 503, 504] {
            assert!(transient_status(
                reqwest::StatusCode::from_u16(code).unwrap()
            ));
        }
        for code in [400, 401, 403, 404, 422, 501, 505] {
            assert!(!transient_status(
                reqwest::StatusCode::from_u16(code).unwrap()
            ));
        }
    }
}

pub fn debug_item(slug: &str) -> Result<()> {
    let url = format!("{BASE_URL}/items/{slug}");

    println!("Fetching {url}");

    let html = fetch_page(&url)?;
    let document = Html::parse_document(&html);

    let tooltip = Selector::parse("[data-item-tooltip=\"true\"]").unwrap();

    let tooltip = document
        .select(&tooltip)
        .next()
        .context("Could not find item tooltip")?;

    let line_selector = Selector::parse(".item-tooltip_line__9hb2F").unwrap();

    println!("\nTooltip lines:");

    for line in tooltip.select(&line_selector) {
        println!("  {:?}", clean_text(&line.text().collect::<String>()));
    }

    Ok(())
}
