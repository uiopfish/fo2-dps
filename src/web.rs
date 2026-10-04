//! Dependency-free local web server for the read-only data explorer and calculators.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::thread;

#[cfg(not(target_arch = "wasm32"))]
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::character::{
    ActiveSkillRole, CharacterBuild, EquipmentSlot, inspect_build, skill_supports_role,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::db::load_skills;
use crate::db::{Item, ItemSet, Skill};

use crate::grinding::{
    GrindingAssumptions, GrindingComparison, GrindingRankingMetric, PriceModel, compare_grinding,
    estimate_grinding, grinding_leaderboard_for_faction,
};
use crate::mobs::Mob;

#[cfg(not(target_arch = "wasm32"))]
const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
pub const WEB_BUILD_ID: &str = "2026-10-03-grinding-share-v29";
const DEFAULT_PAGE_SIZE: usize = 30;
const MAX_PAGE_SIZE: usize = 100;

const INDEX_HTML: &str = include_str!("../web/index.html");
const STYLES_CSS: &str = include_str!("../web/styles.css");
const APP_JS: &str = include_str!("../web/app.js");

pub struct WebData {
    items: Vec<Item>,
    item_sets: Vec<ItemSet>,
    skills: Vec<Skill>,
    mobs: Vec<Mob>,
    item_types: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WebBundle {
    pub schema_version: u32,
    pub web_build_id: String,
    pub items: Vec<Item>,
    pub item_sets: Vec<ItemSet>,
    pub skills: Vec<Skill>,
    pub mobs: Vec<Mob>,
}

impl WebData {
    pub fn new(
        items: Vec<Item>,
        item_sets: Vec<ItemSet>,
        skills: Vec<Skill>,
        mobs: Vec<Mob>,
    ) -> Self {
        let item_types = items
            .iter()
            .map(|item| item.item_type.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self {
            items,
            item_sets,
            skills,
            mobs,
            item_types,
        }
    }

    pub fn from_bundle(bundle: WebBundle) -> std::result::Result<Self, String> {
        if bundle.schema_version != 2 {
            return Err(format!(
                "unsupported web bundle schema version: {}",
                bundle.schema_version
            ));
        }
        if bundle.web_build_id != WEB_BUILD_ID {
            return Err(format!(
                "web bundle build mismatch: expected {WEB_BUILD_ID}, received {}",
                bundle.web_build_id
            ));
        }
        Ok(Self::new(
            bundle.items,
            bundle.item_sets,
            bundle.skills,
            bundle.mobs,
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load() -> Result<Self> {
        let items: Vec<Item> = load_json("data/items.json")?;
        let item_sets = load_json("data/item-sets.json")?;
        let skills = load_skills()?;
        let mobs = load_json("data/mobs.json")?;
        Ok(Self::new(items, item_sets, skills, mobs))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    serde_json::from_slice(&std::fs::read(path)?).with_context(|| format!("Could not load {path}"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn serve(address: &str) -> Result<()> {
    let listener = TcpListener::bind(address)
        .with_context(|| format!("Could not bind web server to http://{address}"))?;
    let data = Arc::new(WebData::load()?);
    println!("FO2 Calculator web app: http://{address}");
    println!("Web build: {WEB_BUILD_ID}");
    println!("Press Ctrl+C to stop. Requests are logged below.");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let data = Arc::clone(&data);
                thread::spawn(move || {
                    if let Err(error) = serve_connection(stream, &data) {
                        eprintln!("Web request failed: {error:#}");
                    }
                });
            }
            Err(error) => eprintln!("Could not accept web connection: {error}"),
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn serve_connection(mut stream: TcpStream, data: &WebData) -> Result<()> {
    let started = std::time::Instant::now();
    let peer = stream
        .peer_addr()
        .map(|address| address.to_string())
        .unwrap_or_else(|_| "unknown".into());
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .context("Could not set web request timeout")?;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    let header_end;
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            bail!("Connection closed before request headers completed");
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            write_response(
                &mut stream,
                Response::json_error(413, "request is too large"),
            )?;
            return Ok(());
        }
        if let Some(index) = find_bytes(&bytes, b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }
    let header_text = std::str::from_utf8(&bytes[..header_end])?.to_owned();
    let content_length = header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    if header_end + content_length > MAX_REQUEST_BYTES {
        write_response(
            &mut stream,
            Response::json_error(413, "request is too large"),
        )?;
        return Ok(());
    }
    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    if bytes.len() < header_end + content_length {
        write_response(
            &mut stream,
            Response::json_error(400, "incomplete request body"),
        )?;
        return Ok(());
    }
    let request_line = header_text.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let body = &bytes[header_end..header_end + content_length];
    let response = route(data, method, target, body);
    eprintln!(
        "[web {WEB_BUILD_ID}] {peer} {method} {target} -> {} ({} bytes, {} ms)",
        response.status,
        response.body.len(),
        started.elapsed().as_millis()
    );
    write_response(&mut stream, response)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

struct Response {
    status: u16,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn text(status: u16, content_type: &'static str, body: &str) -> Self {
        Self {
            status,
            content_type,
            body: body.as_bytes().to_vec(),
        }
    }

    fn json(status: u16, value: Value) -> Self {
        match serde_json::to_vec(&value) {
            Ok(body) => Self {
                status,
                content_type: "application/json; charset=utf-8",
                body,
            },
            Err(error) => Self::json_error(500, &format!("response serialization failed: {error}")),
        }
    }

    fn json_error(status: u16, message: &str) -> Self {
        Self::json(status, json!({ "error": message }))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_response(stream: &mut TcpStream, response: Response) -> Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-FO2-Build: {WEB_BUILD_ID}\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    Ok(())
}

pub fn dispatch_json(
    data: &WebData,
    method: &str,
    target: &str,
    body: &[u8],
) -> std::result::Result<String, String> {
    let response = route(data, method, target, body);
    let body = String::from_utf8(response.body)
        .map_err(|error| format!("response was not valid UTF-8: {error}"))?;
    if response.status == 200 {
        Ok(body)
    } else {
        let message = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| value.get("error")?.as_str().map(str::to_owned))
            .unwrap_or(body);
        Err(message)
    }
}

fn route(data: &WebData, method: &str, target: &str, body: &[u8]) -> Response {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if method == "GET" {
        return match path {
            "/" | "/index.html" => Response::text(200, "text/html; charset=utf-8", INDEX_HTML),
            "/styles.css" => Response::text(200, "text/css; charset=utf-8", STYLES_CSS),
            "/app.js" => Response::text(200, "text/javascript; charset=utf-8", APP_JS),
            "/favicon.ico" => Response {
                status: 200,
                content_type: "image/x-icon",
                body: Vec::new(),
            },
            "/api/summary" => summary(data),
            "/api/items" => list_items(data, query),
            "/api/mobs" => list_mobs(data, query),
            "/api/skills" => list_skills(data, query),
            "/api/item-sets" => list_item_sets(data, query),
            _ if path.starts_with("/api/items/") => item_detail(data, &path[11..]),
            _ if path.starts_with("/api/mobs/") => mob_detail(data, &path[10..]),
            _ if path.starts_with("/api/skills/") => skill_detail(data, &path[12..]),
            _ if path.starts_with("/api/item-sets/") => item_set_detail(data, &path[15..]),
            _ => Response::json_error(404, "route not found"),
        };
    }
    if method == "POST" {
        return match path {
            "/api/build/inspect" => inspect_build_api(data, body),
            "/api/grind/compare" => grind_compare_api(data, body),
            "/api/grind/leaderboard" => grind_leaderboard_api(data, body),

            _ if path.starts_with("/api/grind/") => grind_api(data, &path[11..], body),
            _ => Response::json_error(404, "route not found"),
        };
    }
    Response::json_error(405, "method not allowed")
}

fn summary(data: &WebData) -> Response {
    let factions = data
        .mobs
        .iter()
        .filter(|mob| mob.faction_xp.is_some())
        .filter_map(|mob| mob.faction.as_ref().map(|faction| faction.text.clone()))
        .collect::<BTreeSet<_>>();
    let locations = data
        .mobs
        .iter()
        .flat_map(|mob| &mob.locations)
        .map(|location| location.zone.href.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    Response::json(
        200,
        json!({
            "items": data.items.len(),
            "skills": data.skills.len(),
            "item_sets": data.item_sets.len(),
            "mobs": data.mobs.len(),
            "zones": locations,
            "item_types": data.item_types,
            "factions": factions,
            "web_build_id": WEB_BUILD_ID,
            "capabilities": ["builds", "grinding", "comparisons"]
        }),
    )
}

fn query_params(query: &str) -> BTreeMap<String, String> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            Some((percent_decode(key)?, percent_decode(value)?))
        })
        .collect()
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => output.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let digits = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                output.push(u8::from_str_radix(digits, 16).ok()?);
                index += 2;
            }
            byte => output.push(byte),
        }
        index += 1;
    }
    String::from_utf8(output).ok()
}

fn paging(params: &BTreeMap<String, String>) -> (usize, usize) {
    let offset = params
        .get("offset")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    (offset, limit)
}

fn matches_search(name: &str, slug: &str, search: &str) -> bool {
    search.is_empty()
        || name.to_ascii_lowercase().contains(search)
        || slug.to_ascii_lowercase().contains(search)
}

fn matches_skill_search(skill: &Skill, search: &str) -> bool {
    if matches_search(&skill.name, &skill.slug, search) {
        return true;
    }
    let aliases: &[&str] = match skill.slug.as_str() {
        "firoc-power-297" => &["firoctopus"],
        _ => &[],
    };
    if aliases.iter().any(|alias| alias.contains(search)) {
        return true;
    }
    let Some(rank) = skill.rank else {
        return false;
    };
    let name = skill.name.to_ascii_lowercase();
    format!("{name} {rank}").contains(search)
        || format!("{name} rank {rank}").contains(search)
        || format!("{} {rank}", skill.slug.to_ascii_lowercase()).contains(search)
        || aliases.iter().any(|alias| {
            format!("{alias} {rank}").contains(search)
                || format!("{alias} rank {rank}").contains(search)
        })
}

fn skill_image_url<'a>(data: &'a WebData, skill: &Skill) -> Option<&'a str> {
    let rank = skill.rank?;
    let teaching_prefix = format!("teaches {} rank {rank}.", skill.name.to_ascii_lowercase());
    data.items
        .iter()
        .find(|item| {
            item.image_url.is_some()
                && item.description.as_deref().is_some_and(|description| {
                    description
                        .to_ascii_lowercase()
                        .starts_with(&teaching_prefix)
                })
        })
        .and_then(|item| item.image_url.as_deref())
}

fn page_response(records: Vec<Value>, total: usize, offset: usize, limit: usize) -> Response {
    Response::json(
        200,
        json!({ "records": records, "total": total, "offset": offset, "limit": limit }),
    )
}

fn list_items(data: &WebData, query: &str) -> Response {
    let params = query_params(query);
    let search = params
        .get("search")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let item_type = params.get("type").map(String::as_str).unwrap_or("");
    let slot = params.get("slot").map(String::as_str).unwrap_or("");
    let (offset, limit) = paging(&params);
    let mut filtered: Vec<_> = data
        .items
        .iter()
        .filter(|item| matches_search(&item.name, &item.slug, &search))
        .filter(|item| item_type.is_empty() || item.item_type == item_type)
        .filter(|item| slot.is_empty() || item_accepts_web_slot(item, slot))
        .collect();
    if params
        .get("sort")
        .is_some_and(|sort| sort == "required-level-name")
    {
        filtered.sort_by(|left, right| {
            left.requirements
                .level
                .unwrap_or(0)
                .cmp(&right.requirements.level.unwrap_or(0))
                .then_with(|| {
                    left.name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase())
                })
                .then_with(|| left.slug.cmp(&right.slug))
        });
    }
    let records = filtered
        .iter()
        .skip(offset)
        .take(limit)
        .map(|item| {
            json!({
                "slug": item.slug,
                "name": item.name,
                "item_type": item.item_type,
                "description": &item.description,
                "image_url": &item.image_url,
                "implant_slot": item.implant_slot,
                "damage_min": item.damage_min,
                "damage_max": item.damage_max,
                "attack_speed": item.attack_speed,
                "level_requirement": item.requirements.level,
                "requirements": &item.requirements,
                "stats": &item.stats,
                "market_price": item.market_price,
                "shop_price": item.shop_price
            })
        })
        .collect();
    page_response(records, filtered.len(), offset, limit)
}

fn item_accepts_web_slot(item: &Item, slot: &str) -> bool {
    let equipment_slot = match slot {
        "head" => Some(EquipmentSlot::Head),
        "face" => Some(EquipmentSlot::Face),
        "shoulders" => Some(EquipmentSlot::Shoulders),
        "back" => Some(EquipmentSlot::Back),
        "chest" => Some(EquipmentSlot::Chest),
        "legs" => Some(EquipmentSlot::Legs),
        "main-hand" => Some(EquipmentSlot::MainHand),
        "off-hand" => Some(EquipmentSlot::OffHand),
        "ring" => Some(EquipmentSlot::Ring),
        "trinket" => Some(EquipmentSlot::Trinket),
        "implant-brain" => Some(EquipmentSlot::ImplantBrain),
        "implant-heart" => Some(EquipmentSlot::ImplantHeart),
        "implant-left-arm" => Some(EquipmentSlot::ImplantLeftArm),
        "implant-right-arm" => Some(EquipmentSlot::ImplantRightArm),
        "implant-left-leg" => Some(EquipmentSlot::ImplantLeftLeg),
        "implant-right-leg" => Some(EquipmentSlot::ImplantRightLeg),
        "relic" => Some(EquipmentSlot::Relic),
        "mount" => Some(EquipmentSlot::Mount),
        "guild" => Some(EquipmentSlot::Guild),
        "faction" => Some(EquipmentSlot::Faction),
        "bag" => Some(EquipmentSlot::Bag),
        "fishing-gear" => Some(EquipmentSlot::FishingGear),
        _ => None,
    };
    if let Some(slot) = equipment_slot {
        return slot.accepts_item(item);
    }
    match slot {
        "outfit-head" => item.item_type == "Outfit Head",
        "outfit-face" => item.item_type == "Outfit Face",
        "outfit-shoulders" => item.item_type == "Outfit Shoulders",
        "outfit-back" => item.item_type == "Outfit Back",
        "outfit-chest" => item.item_type == "Outfit Chest",
        "outfit-legs" => item.item_type == "Outfit Legs",
        "outfit-main-hand" => item.item_type == "Outfit Main Hand",
        "outfit-off-hand" => item.item_type == "Outfit Off-Hand",
        "outfit-mount" => item.item_type == "Outfit Mount",
        "outfit-fight-line" => item.item_type == "Outfit Fight Line",
        _ => false,
    }
}

fn list_mobs(data: &WebData, query: &str) -> Response {
    let params = query_params(query);
    let search = params
        .get("search")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let (offset, limit) = paging(&params);
    let filtered: Vec<_> = data
        .mobs
        .iter()
        .filter(|mob| matches_search(&mob.name, &mob.slug, &search))
        .collect();
    let records = filtered
        .iter()
        .skip(offset)
        .take(limit)
        .map(|mob| json!({
            "slug": mob.slug, "name": mob.name, "level": mob.level, "health": mob.health,
            "damage": mob.damage, "attack_speed_ms": mob.attack_speed_ms, "attacks": mob.attacks,
            "locations": mob.locations.len(), "drop_profiles": mob.drop_profiles.len()
        }))
        .collect();
    page_response(records, filtered.len(), offset, limit)
}

fn list_skills(data: &WebData, query: &str) -> Response {
    let params = query_params(query);
    let search = params
        .get("search")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let role = params.get("role").and_then(|role| match role.as_str() {
        "buff" => Some(ActiveSkillRole::Buff),
        "pet" => Some(ActiveSkillRole::Pet),
        "morph" => Some(ActiveSkillRole::Morph),
        _ => None,
    });
    let (offset, limit) = paging(&params);
    let mut filtered: Vec<_> = data
        .skills
        .iter()
        .filter(|skill| matches_skill_search(skill, &search))
        .filter(|skill| role.is_none_or(|role| skill_supports_role(skill, role)))
        .collect();
    filtered.sort_by(|left, right| {
        let level_order = params
            .get("sort")
            .is_some_and(|sort| sort == "required-level-name")
            .then(|| {
                left.level_requirement
                    .unwrap_or(0)
                    .cmp(&right.level_requirement.unwrap_or(0))
            })
            .unwrap_or(std::cmp::Ordering::Equal);
        level_order
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
            .then_with(|| {
                if params
                    .get("sort")
                    .is_some_and(|sort| sort == "required-level-name")
                {
                    left.rank.cmp(&right.rank)
                } else {
                    right.rank.cmp(&left.rank)
                }
            })
            .then_with(|| left.slug.cmp(&right.slug))
    });
    let records = filtered
        .iter()
        .skip(offset)
        .take(limit)
        .map(|skill| json!({
            "slug": skill.slug, "name": skill.name, "rank": skill.rank,
            "level_requirement": skill.level_requirement, "cast_time": skill.cast_time,
            "duration": skill.duration, "cooldown": skill.cooldown, "energy_cost": skill.energy_cost,
            "image_url": skill_image_url(data, skill),
            "effect_types": skill.effects.iter().map(|effect| &effect.effect_type).collect::<Vec<_>>()
        }))
        .collect();
    page_response(records, filtered.len(), offset, limit)
}

fn list_item_sets(data: &WebData, query: &str) -> Response {
    let params = query_params(query);
    let search = params
        .get("search")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let (offset, limit) = paging(&params);
    let filtered: Vec<_> = data
        .item_sets
        .iter()
        .filter(|set| matches_search(&set.name, &set.slug, &search))
        .collect();
    let records = filtered
        .iter()
        .skip(offset)
        .take(limit)
        .map(|set| json!({
            "slug": set.slug, "name": set.name, "pieces": set.pieces.len(), "tiers": set.bonuses.len()
        }))
        .collect();
    page_response(records, filtered.len(), offset, limit)
}

fn item_detail(data: &WebData, slug: &str) -> Response {
    data.items
        .iter()
        .find(|item| item.slug == slug)
        .map(|item| {
            let mut value = serde_json::to_value(item).unwrap();
            value["item_sets"] = json!(
                data.item_sets
                    .iter()
                    .filter(|set| set.pieces.iter().any(|piece| piece.slug == item.slug))
                    .collect::<Vec<_>>()
            );
            Response::json(200, value)
        })
        .unwrap_or_else(|| Response::json_error(404, "item not found"))
}

fn skill_detail(data: &WebData, slug: &str) -> Response {
    data.skills
        .iter()
        .find(|skill| skill.slug == slug)
        .map(|skill| {
            let mut value = serde_json::to_value(skill).unwrap();
            value["image_url"] = json!(skill_image_url(data, skill));
            Response::json(200, value)
        })
        .unwrap_or_else(|| Response::json_error(404, "skill not found"))
}

fn item_set_detail(data: &WebData, slug: &str) -> Response {
    data.item_sets
        .iter()
        .find(|set| set.slug == slug)
        .map(|set| Response::json(200, serde_json::to_value(set).unwrap()))
        .unwrap_or_else(|| Response::json_error(404, "item set not found"))
}

fn mob_detail(data: &WebData, slug: &str) -> Response {
    let Some(mob) = data.mobs.iter().find(|mob| mob.slug == slug) else {
        return Response::json_error(404, "mob not found");
    };
    Response::json(200, compact_mob(mob))
}

fn compact_mob(mob: &Mob) -> Value {
    json!({
        "slug": mob.slug, "name": mob.name, "source_url": mob.source_url, "level": mob.level,
        "health": mob.health, "damage": mob.damage, "attack_speed_ms": mob.attack_speed_ms,
        "attacks": mob.attacks, "faction": mob.faction.as_ref().map(|value| &value.text),
        "faction_xp": mob.faction_xp, "required_weapon": mob.required_weapon.as_ref().map(|value| &value.text),
        "aggressive": mob.aggressive,
        "debuffs": mob.debuffs.iter().map(|value| &value.text).collect::<Vec<_>>(),
        "locations": mob.locations.iter().map(|location| json!({
            "zone": location.zone.text, "zone_href": location.zone.href,
            "map_location_count": location.map_location_count
        })).collect::<Vec<_>>(),
        "drop_profiles": mob.drop_profiles.iter().enumerate().map(|(index, profile)| json!({
            "index": index, "summary": profile.summary.text,
            "zone": profile.zone.as_ref().map(|zone| &zone.text),
            "map_location_count": profile.map_location_count, "solo_coins": profile.solo_coins,
            "drops": profile.drops.iter().map(|drop| json!({
                "item": drop.item.text, "rolls": drop.rolls,
                "solo_chance_at_least_one_percent": drop.solo_chance_at_least_one_percent,
                "maximum_quantity": drop.maximum_quantity
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    })
}

#[derive(Deserialize)]
struct BuildRequest {
    build: CharacterBuild,
}

#[derive(Deserialize)]
struct GrindRequest {
    build: CharacterBuild,
    assumptions: GrindingAssumptions,
}

#[derive(Deserialize)]
struct ComparisonRequest {
    build: CharacterBuild,
    comparison: GrindingComparison,
}

#[derive(Deserialize)]
struct LeaderboardRequest {
    build: CharacterBuild,
    assumptions: GrindingAssumptions,
    ranking_metric: GrindingRankingMetric,
    limit: usize,
    #[serde(default)]
    maximum_loot_clicks_per_hour: Option<f64>,
    #[serde(default)]
    faction: Option<String>,
}

fn parse_body<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, Response> {
    serde_json::from_slice(body)
        .map_err(|error| Response::json_error(400, &format!("invalid JSON request: {error}")))
}

fn inspect_build_api(data: &WebData, body: &[u8]) -> Response {
    let build = match serde_json::from_slice::<CharacterBuild>(body) {
        Ok(build) => build,
        Err(_) => match parse_body::<BuildRequest>(body) {
            Ok(request) => request.build,
            Err(response) => return response,
        },
    };
    let inspection = inspect_build(&build, &data.items, &data.item_sets, &data.skills);
    Response::json(200, json!({ "build": inspection }))
}

fn grind_api(data: &WebData, mob_slug: &str, body: &[u8]) -> Response {
    let mut request: GrindRequest = match parse_body(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let Some(mob) = data.mobs.iter().find(|mob| mob.slug == mob_slug) else {
        return Response::json_error(404, "mob not found");
    };
    let inspection = inspect_build(&request.build, &data.items, &data.item_sets, &data.skills);
    if !inspection.is_valid() {
        return Response::json(
            422,
            json!({ "error": "build validation failed", "build": inspection }),
        );
    }
    request
        .assumptions
        .encounter
        .apply_build_defense(inspection.level, inspection.confirmed_derived_stats.armor);
    match estimate_grinding(mob, &data.items, request.assumptions) {
        Ok(estimate) => Response::json(200, json!({ "build": inspection, "grinding": estimate })),
        Err(error) => Response::json_error(422, &error),
    }
}

fn grind_leaderboard_api(data: &WebData, body: &[u8]) -> Response {
    let mut request: LeaderboardRequest = match parse_body(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let inspection = inspect_build(&request.build, &data.items, &data.item_sets, &data.skills);
    if !inspection.is_valid() {
        return Response::json(
            422,
            json!({ "error": "build validation failed", "build": inspection }),
        );
    }
    request
        .assumptions
        .encounter
        .apply_build_defense(inspection.level, inspection.confirmed_derived_stats.armor);
    let limit = request.limit.min(50);
    let requested_faction = request
        .faction
        .as_deref()
        .map(str::trim)
        .filter(|faction| !faction.is_empty());
    let faction = requested_faction.and_then(|requested| {
        data.mobs
            .iter()
            .filter_map(|mob| mob.faction.as_ref())
            .find(|faction| faction.text.eq_ignore_ascii_case(requested))
            .map(|faction| faction.text.as_str())
    });
    if requested_faction.is_some() && faction.is_none() {
        return Response::json_error(422, "unknown faction filter");
    }
    if request.ranking_metric == GrindingRankingMetric::FactionXpPerHour && faction.is_none() {
        return Response::json_error(422, "faction XP ranking requires a faction filter");
    }
    request.assumptions.price_model = PriceModel::Shop;
    match grinding_leaderboard_for_faction(
        &data.mobs,
        &data.items,
        request.assumptions,
        request.ranking_metric,
        limit,
        request.maximum_loot_clicks_per_hour,
        faction,
    ) {
        Ok(leaderboard) => Response::json(
            200,
            json!({ "build": inspection, "leaderboard": leaderboard }),
        ),
        Err(error) => Response::json_error(422, &error),
    }
}

fn grind_compare_api(data: &WebData, body: &[u8]) -> Response {
    let mut request: ComparisonRequest = match parse_body(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let inspection = inspect_build(&request.build, &data.items, &data.item_sets, &data.skills);
    if !inspection.is_valid() {
        return Response::json(
            422,
            json!({ "error": "build validation failed", "build": inspection }),
        );
    }
    for target in &mut request.comparison.targets {
        target
            .assumptions
            .encounter
            .apply_build_defense(inspection.level, inspection.confirmed_derived_stats.armor);
    }
    match compare_grinding(&data.mobs, &data.items, request.comparison) {
        Ok(comparison) => Response::json(
            200,
            json!({ "build": inspection, "comparison": comparison }),
        ),
        Err(error) => Response::json_error(422, &error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{SkillEffect, SkillEffectComponent};

    fn data() -> WebData {
        WebData {
            items: vec![Item {
                name: "Toy Sword".into(),
                slug: "toy-sword-26".into(),
                item_type: "One-Hand Sword".into(),
                description: None,
                image_url: None,
                implant_slot: None,
                damage_min: Some(4),
                damage_max: Some(8),
                attack_speed: Some(1.6),
                stats: Default::default(),
                requirements: Default::default(),
                market_price: Some(100),
                recently_sold_price: None,
                shop_price: Some(25),
                sellable_to_shops: true,
                rebirth: false,
                ascension: false,
                unparsed_tooltip_lines: Vec::new(),
            }],
            item_sets: Vec::new(),
            skills: Vec::new(),
            mobs: Vec::new(),
            item_types: vec!["One-Hand Sword".into()],
        }
    }

    #[test]
    fn static_routes_and_summary_are_available() {
        let data = data();
        assert_eq!(route(&data, "GET", "/", &[]).status, 200);
        let response = route(&data, "GET", "/api/summary", &[]);
        assert_eq!(response.status, 200);
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["items"], 1);
        assert!(
            !value["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("encounters"))
        );
        assert_eq!(
            route(&data, "POST", "/api/encounter/angry-skele-8", b"{}").status,
            404
        );
    }

    #[test]
    fn item_search_filters_and_paginates() {
        let initial_data = data();
        let response = route(
            &initial_data,
            "GET",
            "/api/items?search=toy&type=One-Hand+Sword&slot=main-hand&limit=10",
            &[],
        );
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["total"], 1);
        assert_eq!(value["records"][0]["slug"], "toy-sword-26");

        let mut data = data();
        let item_bytes = serde_json::to_vec(&data.items[0]).unwrap();
        let copy_item = || serde_json::from_slice::<Item>(&item_bytes).unwrap();
        let mut high_level = copy_item();
        high_level.name = "Alpha Sword".into();
        high_level.slug = "alpha-sword".into();
        high_level.requirements.level = Some(20);
        let mut low_level_zeta = copy_item();
        low_level_zeta.name = "Zeta Sword".into();
        low_level_zeta.slug = "zeta-sword".into();
        low_level_zeta.requirements.level = Some(5);
        let mut low_level_alpha = copy_item();
        low_level_alpha.name = "Beta Sword".into();
        low_level_alpha.slug = "beta-sword".into();
        low_level_alpha.requirements.level = Some(5);
        data.items = vec![high_level, low_level_zeta, low_level_alpha];
        let response = route(
            &data,
            "GET",
            "/api/items?slot=main-hand&sort=required-level-name&limit=2",
            &[],
        );
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["total"], 3);
        assert_eq!(value["records"][0]["slug"], "beta-sword");
        assert_eq!(value["records"][1]["slug"], "zeta-sword");
    }

    #[test]
    fn skill_search_filters_active_roles_and_returns_rank() {
        fn skill(slug: &str, rank: u32, component: SkillEffectComponent) -> Skill {
            Skill {
                name: slug.into(),
                slug: slug.into(),
                rank: Some(rank),
                quick_facts: Vec::new(),
                level_requirement: None,
                attribute_requirements: Vec::new(),
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

        let mut firoc_power = skill("firoc-power-297", 1, SkillEffectComponent::Morph);
        firoc_power.effects[0]
            .components
            .push(SkillEffectComponent::TimedStatModifier {
                stat: "Attack power".into(),
                value: 700,
            });
        let mut ruse = skill("ruse-352", 3, SkillEffectComponent::Morph);
        ruse.effects[0]
            .components
            .push(SkillEffectComponent::TimedStatModifier {
                stat: "Crit".into(),
                value: 20,
            });
        let mut data = data();
        data.items[0].description = Some("Teaches buff-rank-2 Rank 2.".into());
        data.items[0].image_url = Some("https://example.test/buff-rank-2.png".into());
        data.skills = vec![
            skill(
                "buff-rank-2",
                2,
                SkillEffectComponent::TimedStatModifier {
                    stat: "Crit".into(),
                    value: 5,
                },
            ),
            skill("pet-rank-1", 1, SkillEffectComponent::Pet),
            firoc_power,
            ruse,
        ];
        data.skills[0].level_requirement = Some(40);
        data.skills[2].level_requirement = Some(10);
        data.skills[3].level_requirement = Some(10);
        let response = route(
            &data,
            "GET",
            "/api/skills?role=buff&search=buff-rank-2+2&limit=100",
            &[],
        );
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["total"], 1);
        assert_eq!(value["records"][0]["slug"], "buff-rank-2");
        assert_eq!(value["records"][0]["rank"], 2);
        assert_eq!(
            value["records"][0]["image_url"],
            "https://example.test/buff-rank-2.png"
        );
        let detail = route(&data, "GET", "/api/skills/buff-rank-2", &[]);
        let detail: Value = serde_json::from_slice(&detail.body).unwrap();
        assert_eq!(detail["image_url"], "https://example.test/buff-rank-2.png");

        let response = route(
            &data,
            "GET",
            "/api/skills?role=buff&search=firoctopus&limit=100",
            &[],
        );
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["total"], 1);
        assert_eq!(value["records"][0]["slug"], "firoc-power-297");

        let response = route(
            &data,
            "GET",
            "/api/skills?role=buff&search=ruse&limit=100",
            &[],
        );
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["total"], 1);
        assert_eq!(value["records"][0]["slug"], "ruse-352");

        assert_eq!(
            route(&data, "GET", "/api/skills/buff-rank-2", &[]).status,
            200
        );

        let response = route(
            &data,
            "GET",
            "/api/skills?role=buff&sort=required-level-name&limit=2",
            &[],
        );
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["total"], 3);
        assert_eq!(value["records"][0]["slug"], "firoc-power-297");
        assert_eq!(value["records"][1]["slug"], "ruse-352");
    }

    #[test]
    fn detail_routes_use_slug_without_a_leading_slash() {
        let mut data = data();
        data.item_sets.push(ItemSet {
            name: "Set".into(),
            slug: "set-1".into(),
            bonus_rule: String::new(),
            resource_tool_rule: String::new(),
            pieces: Vec::new(),
            bonuses: Vec::new(),
        });
        assert_eq!(
            route(&data, "GET", "/api/items/toy-sword-26", &[]).status,
            200
        );
        assert_eq!(route(&data, "GET", "/api/item-sets/set-1", &[]).status, 200);
    }

    #[test]
    fn invalid_json_and_unknown_routes_are_structured_errors() {
        let data = data();
        assert_eq!(route(&data, "POST", "/api/build/inspect", b"{").status, 400);
        assert_eq!(route(&data, "GET", "/api/nope", &[]).status, 404);
        assert_eq!(route(&data, "DELETE", "/api/items", &[]).status, 405);
    }
}
