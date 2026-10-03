//! Location-specific grinding and economy estimates.
//!
//! Encounter time comes from the expected-event model. Travel, recovery,
//! respawn, quantity, coin, and valuation policies are explicit scenario inputs.

use std::collections::BTreeMap;

use crate::db::Item;
use crate::encounter::{
    EncounterAssumptions, EncounterEstimate, EncounterOutcome, estimate_encounter,
};
use crate::mobs::{Mob, MobDrop, MobDropProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoinExpectationModel {
    MidpointOfPublishedRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropQuantityModel {
    OnePerSuccessfulRoll,
    UniformInclusiveOneToPublishedMaximum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceModel {
    None,
    Shop,
    MarketObservation,
    RecentlySoldObservation,
    ShopOrRecentlySold,
}

const REGULAR_MOB_RESPAWN_SECONDS: f64 = 30.0;

fn default_recently_sold_threshold_multiplier() -> f64 {
    3.0
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GrindingAssumptions {
    pub encounter: EncounterAssumptions,
    pub drop_profile_index: usize,
    pub travel_seconds_per_kill: f64,
    pub recovery_seconds_per_kill: f64,
    /// Explicit route wait after travel/recovery assumptions.
    pub respawn_wait_seconds_per_kill: f64,
    /// Opt-in inference: each published map location is one independent 30-second spawn.
    #[serde(default)]
    pub apply_inferred_spawn_cap: bool,
    pub coin_expectation_model: CoinExpectationModel,
    /// Retained for saved-JSON compatibility; calculations always normalize this
    /// to one item per successful independent roll.
    pub drop_quantity_model: Option<DropQuantityModel>,
    pub price_model: PriceModel,
    #[serde(default = "default_recently_sold_threshold_multiplier")]
    pub recently_sold_threshold_multiplier: f64,
    #[serde(default)]
    pub consumables: Vec<ConsumableAssumption>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ConsumableAssumption {
    pub label: String,
    pub expected_quantity_per_kill: f64,
    pub unit_cost: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ConsumableEstimate {
    pub label: String,
    pub expected_quantity_per_kill: f64,
    pub unit_cost: f64,
    pub expected_cost_per_kill: f64,
    pub expected_cost_per_hour: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CoinEstimate {
    pub published_min_per_kill: u64,
    pub published_max_per_kill: u64,
    pub expected_per_kill: f64,
    pub expected_per_hour: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropValuationSource {
    Shop,
    MarketObservation,
    RecentlySoldObservation,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DropEstimate {
    pub item_label: String,
    pub item_slug: Option<String>,
    pub independent_rolls: usize,
    pub expected_successful_rolls_per_kill: Option<f64>,
    pub published_chance_at_least_one_percent: Option<f64>,
    pub published_maximum_quantity: Option<u64>,
    pub expected_quantity_per_kill: Option<f64>,
    pub expected_quantity_per_hour: Option<f64>,
    pub selected_unit_price: Option<u64>,
    pub valuation_source: Option<DropValuationSource>,
    pub expected_value_per_kill: Option<f64>,
    pub expected_value_per_hour: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrindingEstimate {
    pub mob_slug: String,
    pub mob_faction_label: Option<String>,
    pub faction_xp_per_kill: Option<i64>,
    pub expected_faction_xp_per_hour: Option<f64>,
    pub drop_profile_index: usize,
    pub profile_summary: String,
    pub zone: Option<String>,
    pub published_map_location_count: Option<u64>,
    pub encounter: EncounterEstimate,
    pub travel_seconds_per_kill: f64,
    pub recovery_seconds_per_kill: f64,
    pub respawn_wait_seconds_per_kill: f64,
    pub cycle_seconds: f64,
    pub uncapped_kills_per_hour: f64,
    pub inferred_spawn_cap_kills_per_hour: Option<f64>,
    pub spawn_cap_applied: bool,
    pub kills_per_hour: f64,
    pub expected_loot_clicks_per_kill: Option<f64>,
    pub expected_loot_clicks_per_hour: Option<f64>,
    pub coins: Option<CoinEstimate>,
    pub drops: Vec<DropEstimate>,
    pub expected_item_value_per_kill: Option<f64>,
    pub expected_item_value_per_hour: Option<f64>,
    pub expected_total_coins_and_items_per_hour: Option<f64>,
    pub consumables: Vec<ConsumableEstimate>,
    pub expected_consumable_cost_per_kill: f64,
    pub expected_consumable_cost_per_hour: f64,
    pub expected_net_value_per_kill: Option<f64>,
    pub expected_net_value_per_hour: Option<f64>,
    pub assumptions: GrindingAssumptions,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GrindingRankingMetric {
    KillsPerHour,
    FactionXpPerHour,
    ExpectedCoinsPerHour,
    ExpectedItemValuePerHour,
    ExpectedGrossValuePerHour,
    ExpectedNetValuePerHour,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GrindingTarget {
    pub mob_slug: String,
    pub assumptions: GrindingAssumptions,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GrindingComparison {
    pub ranking_metric: GrindingRankingMetric,
    pub targets: Vec<GrindingTarget>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrindingComparisonEntry {
    pub rank: Option<usize>,
    pub mob_slug: String,
    pub ranking_value: Option<f64>,
    pub estimate: Option<GrindingEstimate>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrindingComparisonResult {
    pub ranking_metric: GrindingRankingMetric,
    pub entries: Vec<GrindingComparisonEntry>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrindingLeaderboardRow {
    pub rank: usize,
    pub mob_slug: String,
    pub mob_name: String,
    pub mob_level: Option<u64>,
    pub ranking_value: f64,
    pub known_gold_per_hour: Option<f64>,
    pub estimate: GrindingEstimate,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrindingLeaderboardResult {
    pub ranking_metric: GrindingRankingMetric,
    pub faction_filter: Option<String>,
    pub total_mob_count: usize,
    pub evaluated_mob_count: usize,
    pub faction_filtered_mob_count: usize,
    pub ranked_mob_count: usize,
    pub skipped_mob_count: usize,
    pub excluded_resource_mob_count: usize,
    pub excluded_boss_candidate_count: usize,
    pub evaluated_route_count: usize,
    pub failed_route_count: usize,
    pub unrankable_route_count: usize,
    pub click_filtered_route_count: usize,
    pub rows: Vec<GrindingLeaderboardRow>,
    pub notes: Vec<String>,
}

pub fn compare_grinding(
    mobs: &[Mob],
    items: &[Item],
    comparison: GrindingComparison,
) -> Result<GrindingComparisonResult, String> {
    if comparison.targets.is_empty() {
        return Err("grinding comparison requires at least one target".into());
    }
    let mob_by_slug: BTreeMap<_, _> = mobs.iter().map(|mob| (mob.slug.as_str(), mob)).collect();
    let mut entries: Vec<_> = comparison
        .targets
        .into_iter()
        .map(|target| {
            let result = mob_by_slug
                .get(target.mob_slug.as_str())
                .ok_or_else(|| "mob slug is absent from the mob snapshot".to_string())
                .and_then(|mob| estimate_grinding(mob, items, target.assumptions));
            match result {
                Ok(estimate) => GrindingComparisonEntry {
                    rank: None,
                    mob_slug: target.mob_slug,
                    ranking_value: ranking_value(&estimate, comparison.ranking_metric),
                    estimate: Some(estimate),
                    error: None,
                },
                Err(error) => GrindingComparisonEntry {
                    rank: None,
                    mob_slug: target.mob_slug,
                    ranking_value: None,
                    estimate: None,
                    error: Some(error),
                },
            }
        })
        .collect();
    let mut ranked: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| entry.ranking_value.map(|value| (index, value)))
        .collect();
    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
    for (position, (index, _)) in ranked.into_iter().enumerate() {
        entries[index].rank = Some(position + 1);
    }
    entries.sort_by_key(|entry| entry.rank.unwrap_or(usize::MAX));

    Ok(GrindingComparisonResult {
        ranking_metric: comparison.ranking_metric,
        entries,
        notes: vec![
            "Only entries with a known selected metric are ranked; unknown values and failed scenarios remain unranked rather than becoming zero.".into(),
            "Each target retains its own encounter, location, downtime, quantity, price, and consumable assumptions.".into(),
        ],
    })
}

fn ranking_value(estimate: &GrindingEstimate, metric: GrindingRankingMetric) -> Option<f64> {
    match metric {
        GrindingRankingMetric::KillsPerHour => Some(estimate.kills_per_hour),
        GrindingRankingMetric::FactionXpPerHour => estimate.expected_faction_xp_per_hour,
        GrindingRankingMetric::ExpectedCoinsPerHour => {
            estimate.coins.as_ref().map(|coins| coins.expected_per_hour)
        }
        GrindingRankingMetric::ExpectedItemValuePerHour => estimate.expected_item_value_per_hour,
        GrindingRankingMetric::ExpectedGrossValuePerHour => {
            estimate.expected_total_coins_and_items_per_hour
        }
        GrindingRankingMetric::ExpectedNetValuePerHour => estimate.expected_net_value_per_hour,
    }
}

fn is_resource_target(mob: &Mob) -> bool {
    matches!(
        mob.required_weapon
            .as_ref()
            .map(|weapon| weapon.text.as_str()),
        Some("Mining tool" | "Unlocking tool" | "Wood-cutting tool")
    )
}

fn is_boss_candidate(mob: &Mob) -> bool {
    mob.raw.source_html.contains("achievement-boss-")
}

fn known_gold_per_hour(estimate: &GrindingEstimate) -> Option<f64> {
    let known_coin_value = estimate.coins.as_ref().map(|coins| coins.expected_per_hour);
    let known_drop_values: Vec<_> = estimate
        .drops
        .iter()
        .filter_map(|drop| drop.expected_value_per_hour)
        .collect();
    if known_coin_value.is_none() && known_drop_values.is_empty() {
        return None;
    }

    Some(
        known_coin_value.unwrap_or(0.0) + known_drop_values.into_iter().sum::<f64>()
            - estimate.expected_consumable_cost_per_hour,
    )
}

#[cfg(test)]
pub fn grinding_leaderboard(
    mobs: &[Mob],
    items: &[Item],
    base_assumptions: GrindingAssumptions,
    ranking_metric: GrindingRankingMetric,
    limit: usize,
    maximum_loot_clicks_per_hour: Option<f64>,
) -> Result<GrindingLeaderboardResult, String> {
    grinding_leaderboard_for_faction(
        mobs,
        items,
        base_assumptions,
        ranking_metric,
        limit,
        maximum_loot_clicks_per_hour,
        None,
    )
}

pub fn grinding_leaderboard_for_faction(
    mobs: &[Mob],
    items: &[Item],
    base_assumptions: GrindingAssumptions,
    ranking_metric: GrindingRankingMetric,
    limit: usize,
    maximum_loot_clicks_per_hour: Option<f64>,
    faction_filter: Option<&str>,
) -> Result<GrindingLeaderboardResult, String> {
    if limit == 0 {
        return Err("grinding leaderboard limit must be greater than zero".into());
    }
    if let Some(maximum) = maximum_loot_clicks_per_hour {
        nonnegative_finite(maximum, "maximum_loot_clicks_per_hour")?;
    }

    let faction_filter = faction_filter
        .map(str::trim)
        .filter(|faction| !faction.is_empty());
    let faction_matches = |mob: &Mob| {
        faction_filter.is_none_or(|selected| {
            mob.faction
                .as_ref()
                .is_some_and(|faction| faction.text.eq_ignore_ascii_case(selected))
        })
    };
    let total_mob_count = mobs.len();
    let evaluated_mob_count = mobs.iter().filter(|mob| faction_matches(mob)).count();
    let faction_filtered_mob_count = total_mob_count - evaluated_mob_count;
    let mut rows = Vec::new();
    let mut notes = Vec::new();
    let mut evaluated_route_count = 0usize;
    let mut failed_route_count = 0usize;
    let mut unrankable_route_count = 0usize;
    let mut excluded_resource_mob_count = 0usize;
    let mut excluded_boss_candidate_count = 0usize;
    let mut click_filtered_route_count = 0usize;

    for mob in mobs {
        if !faction_matches(mob) {
            continue;
        }
        if is_resource_target(mob) {
            excluded_resource_mob_count += 1;
            continue;
        }
        if is_boss_candidate(mob) {
            excluded_boss_candidate_count += 1;
            continue;
        }

        let mut best: Option<(f64, GrindingEstimate)> = None;

        for drop_profile_index in 0..mob.drop_profiles.len() {
            evaluated_route_count += 1;
            let mut assumptions = base_assumptions.clone();
            assumptions.drop_profile_index = drop_profile_index;
            match estimate_grinding(mob, items, assumptions) {
                Ok(estimate) => {
                    if maximum_loot_clicks_per_hour.is_some_and(|maximum| {
                        estimate
                            .expected_loot_clicks_per_hour
                            .is_none_or(|clicks| clicks > maximum)
                    }) {
                        click_filtered_route_count += 1;
                        continue;
                    }
                    match if ranking_metric == GrindingRankingMetric::ExpectedNetValuePerHour {
                        known_gold_per_hour(&estimate)
                    } else {
                        ranking_value(&estimate, ranking_metric)
                    } {
                        Some(value) if value.is_finite() => {
                            if best
                                .as_ref()
                                .is_none_or(|(best_value, _)| value > *best_value)
                            {
                                best = Some((value, estimate));
                            }
                        }
                        _ => {
                            unrankable_route_count += 1;
                        }
                    }
                }
                Err(_) => {
                    failed_route_count += 1;
                }
            }
        }

        if let Some((value, estimate)) = best {
            rows.push(GrindingLeaderboardRow {
                rank: 0,
                mob_slug: mob.slug.clone(),
                mob_name: mob.name.clone(),
                mob_level: mob.level,
                ranking_value: value,
                known_gold_per_hour: known_gold_per_hour(&estimate),
                estimate,
            });
        }
    }

    rows.sort_by(|left, right| {
        right
            .ranking_value
            .total_cmp(&left.ranking_value)
            .then_with(|| left.mob_slug.cmp(&right.mob_slug))
    });
    let ranked_mob_count = rows.len();
    let skipped_mob_count = evaluated_mob_count.saturating_sub(ranked_mob_count);
    rows.truncate(limit);
    for (index, row) in rows.iter_mut().enumerate() {
        row.rank = index + 1;
    }
    notes.insert(
        0,
        match faction_filter {
            Some(faction) => format!(
                "Restricted the leaderboard to {evaluated_mob_count} {faction} mobs; {faction_filtered_mob_count} mobs from other or unknown factions were excluded before route evaluation."
            ),
            None => format!("No faction filter was applied; all {total_mob_count} mobs were considered."),
        },
    );
    notes.insert(
        1,
        format!(
            "Evaluated {evaluated_route_count} drop-profile routes: {failed_route_count} failed estimation and {unrankable_route_count} had an unknown or non-finite selected metric."
        ),
    );
    notes.insert(
        2,
        format!(
            "Excluded {excluded_resource_mob_count} mining, unlocking, or wood-cutting targets and {excluded_boss_candidate_count} boss candidates. Boss candidates are detected from achievement artwork and remain excluded because their 30-minute respawn is outside the sustained-grinding ranking."
        ),
    );
    notes.insert(
        3,
        "Each mob is represented by its successful drop-profile route with the highest selected metric; ties use the earliest profile, and leaderboard ties are ordered by mob slug.".into(),
    );
    notes.insert(
        4,
        "known_gold_per_hour is an MVP subtotal of known expected coin and drop value per hour minus consumable cost per hour; unknown drop values contribute zero. ExpectedNetValuePerHour leaderboard ranking uses this subtotal without changing the detailed estimate's strict null propagation.".into(),
    );
    if let Some(maximum) = maximum_loot_clicks_per_hour {
        notes.push(format!(
            "Filtered {click_filtered_route_count} routes whose expected loot clicks were unknown or exceeded the {maximum} clicks/hour limit."
        ));
    }
    if ranked_mob_count > rows.len() {
        notes.push(format!(
            "Returned the top {} of {ranked_mob_count} ranked mobs due to the requested limit.",
            rows.len()
        ));
    }

    Ok(GrindingLeaderboardResult {
        ranking_metric,
        faction_filter: faction_filter.map(str::to_owned),
        total_mob_count,
        evaluated_mob_count,
        faction_filtered_mob_count,
        ranked_mob_count,
        skipped_mob_count,
        excluded_resource_mob_count,
        excluded_boss_candidate_count,
        evaluated_route_count,
        failed_route_count,
        unrankable_route_count,
        click_filtered_route_count,
        rows,
        notes,
    })
}

pub fn estimate_grinding(
    mob: &Mob,
    items: &[Item],
    mut assumptions: GrindingAssumptions,
) -> Result<GrindingEstimate, String> {
    nonnegative_finite(
        assumptions.travel_seconds_per_kill,
        "travel_seconds_per_kill",
    )?;
    nonnegative_finite(
        assumptions.recovery_seconds_per_kill,
        "recovery_seconds_per_kill",
    )?;
    nonnegative_finite(
        assumptions.respawn_wait_seconds_per_kill,
        "respawn_wait_seconds_per_kill",
    )?;
    if !assumptions.recently_sold_threshold_multiplier.is_finite()
        || assumptions.recently_sold_threshold_multiplier < 1.0
    {
        return Err("recently_sold_threshold_multiplier must be finite and at least 1".into());
    }
    assumptions.drop_quantity_model = Some(DropQuantityModel::OnePerSuccessfulRoll);
    for consumable in &assumptions.consumables {
        nonnegative_finite(
            consumable.expected_quantity_per_kill,
            "consumable expected_quantity_per_kill",
        )?;
        nonnegative_finite(consumable.unit_cost, "consumable unit_cost")?;
    }
    let encounter = estimate_encounter(mob, assumptions.encounter.clone())?;
    if encounter.expected_event_timeline.outcome != EncounterOutcome::MobDefeated {
        return Err("grinding requires an encounter scenario where the mob is defeated".into());
    }
    let profile = mob
        .drop_profiles
        .get(assumptions.drop_profile_index)
        .ok_or_else(|| {
            format!(
                "drop_profile_index {} is out of range for {} profiles",
                assumptions.drop_profile_index,
                mob.drop_profiles.len()
            )
        })?;
    let combat_cycle_seconds = encounter.expected_event_timeline.elapsed_seconds;
    let cycle_seconds = combat_cycle_seconds
        + assumptions.travel_seconds_per_kill
        + assumptions.recovery_seconds_per_kill
        + assumptions.respawn_wait_seconds_per_kill;
    let inferred_spawn_cap_kills_per_hour = profile
        .map_location_count
        .filter(|count| *count > 0)
        .map(|count| count as f64 * 3600.0 / REGULAR_MOB_RESPAWN_SECONDS);
    let zero_cycle_uses_spawn_capacity = cycle_seconds == 0.0;
    let uncapped_kills_per_hour = if zero_cycle_uses_spawn_capacity {
        inferred_spawn_cap_kills_per_hour.ok_or_else(|| {
            "zero-second grinding cycle requires a positive published spawn count".to_string()
        })?
    } else {
        3600.0 / cycle_seconds
    };
    let spawn_cap_enabled = assumptions.apply_inferred_spawn_cap || zero_cycle_uses_spawn_capacity;
    let kills_per_hour = if spawn_cap_enabled {
        inferred_spawn_cap_kills_per_hour
            .map(|cap| uncapped_kills_per_hour.min(cap))
            .unwrap_or(uncapped_kills_per_hour)
    } else {
        uncapped_kills_per_hour
    };
    let spawn_cap_applied =
        zero_cycle_uses_spawn_capacity || kills_per_hour < uncapped_kills_per_hour;
    let coins = coin_estimate(profile, kills_per_hour, assumptions.coin_expectation_model);
    let item_by_slug: BTreeMap<_, _> = items
        .iter()
        .map(|item| (item.slug.as_str(), item))
        .collect();
    let drops: Vec<_> = profile
        .drops
        .iter()
        .map(|drop| {
            drop_estimate(
                drop,
                &item_by_slug,
                kills_per_hour,
                assumptions.price_model,
                assumptions.recently_sold_threshold_multiplier,
            )
        })
        .collect();

    let expected_loot_clicks_per_kill = drops.iter().try_fold(0.0, |sum, drop| {
        Some(sum + drop.expected_successful_rolls_per_kill?)
    });
    let expected_loot_clicks_per_hour =
        expected_loot_clicks_per_kill.map(|clicks| clicks * kills_per_hour);

    let all_values_known = drops
        .iter()
        .all(|drop| drop.expected_value_per_kill.is_some());
    let item_value_per_kill = all_values_known.then(|| {
        drops
            .iter()
            .filter_map(|drop| drop.expected_value_per_kill)
            .sum::<f64>()
    });
    let item_value_per_hour = item_value_per_kill.map(|value| value * kills_per_hour);
    let total_per_kill = match (coins.as_ref(), item_value_per_kill) {
        (Some(coins), Some(items)) => Some(coins.expected_per_kill + items),
        _ => None,
    };
    let total_per_hour = total_per_kill.map(|value| value * kills_per_hour);
    let consumables: Vec<_> = assumptions
        .consumables
        .iter()
        .map(|consumable| {
            let per_kill = consumable.expected_quantity_per_kill * consumable.unit_cost;
            ConsumableEstimate {
                label: consumable.label.clone(),
                expected_quantity_per_kill: consumable.expected_quantity_per_kill,
                unit_cost: consumable.unit_cost,
                expected_cost_per_kill: per_kill,
                expected_cost_per_hour: per_kill * kills_per_hour,
            }
        })
        .collect();
    let consumable_cost_per_kill = consumables
        .iter()
        .map(|consumable| consumable.expected_cost_per_kill)
        .sum::<f64>();
    let consumable_cost_per_hour = consumable_cost_per_kill * kills_per_hour;
    let net_per_kill = total_per_kill.map(|value| value - consumable_cost_per_kill);
    let net_per_hour = net_per_kill.map(|value| value * kills_per_hour);

    Ok(GrindingEstimate {
        mob_slug: mob.slug.clone(),
        mob_faction_label: mob.faction.as_ref().map(|faction| faction.text.clone()),
        faction_xp_per_kill: mob.faction_xp,
        expected_faction_xp_per_hour: mob
            .faction_xp
            .map(|faction_xp| faction_xp as f64 * kills_per_hour),
        drop_profile_index: assumptions.drop_profile_index,
        profile_summary: profile.summary.text.clone(),
        zone: profile.zone.as_ref().map(|zone| zone.text.clone()),
        published_map_location_count: profile.map_location_count,
        encounter,
        travel_seconds_per_kill: assumptions.travel_seconds_per_kill,
        recovery_seconds_per_kill: assumptions.recovery_seconds_per_kill,
        respawn_wait_seconds_per_kill: assumptions.respawn_wait_seconds_per_kill,
        cycle_seconds,
        uncapped_kills_per_hour,
        inferred_spawn_cap_kills_per_hour,
        spawn_cap_applied,
        kills_per_hour,
        expected_loot_clicks_per_kill,
        expected_loot_clicks_per_hour,
        coins,
        drops,
        expected_item_value_per_kill: item_value_per_kill,
        expected_item_value_per_hour: item_value_per_hour,
        expected_total_coins_and_items_per_hour: total_per_hour,
        consumables,
        expected_consumable_cost_per_kill: consumable_cost_per_kill,
        expected_consumable_cost_per_hour: consumable_cost_per_hour,
        expected_net_value_per_kill: net_per_kill,
        expected_net_value_per_hour: net_per_hour,
        assumptions,
        notes: vec![
            if zero_cycle_uses_spawn_capacity {
                "An opening one-shot has zero combat-cycle time because the attack interval resets on kill; with no route overhead, kills per hour equal published spawn count × 120.".into()
            } else {
                "Combat time uses the encounter timeline directly; travel, recovery, and explicit route wait are added per kill.".into()
            },
            if zero_cycle_uses_spawn_capacity {
                "Spawn capacity is the automatic finite bound for this zero-second cycle, even though the optional cap setting is disabled.".into()
            } else if spawn_cap_enabled {
                "Spawn cap enabled: each published map spawn has a 30-second respawn, giving spawn count × 120 kills/hour.".into()
            } else {
                "Spawn cap disabled for this positive-duration route.".into()
            },
            "Every published independent drop roll remains separate; each successful roll produces one item and one expected loot click.".into(),
            "Market and recently-sold prices are observations, not guaranteed proceeds; shop prices are fixed source values but availability and transaction costs are not modeled.".into(),
            "Consumable costs are explicit expected quantities and unit costs; implicit durability, repair, and opportunity costs are not inferred.".into(),
            "Grinding estimates omit competition, downtime variance, inventory limits, death recovery, taxes, and price liquidity.".into(),
        ],
    })
}

fn coin_estimate(
    profile: &MobDropProfile,
    kills_per_hour: f64,
    model: CoinExpectationModel,
) -> Option<CoinEstimate> {
    let range = profile.solo_coins.as_ref()?;
    let expected = match model {
        CoinExpectationModel::MidpointOfPublishedRange => {
            (range.min as f64 + range.max as f64) / 2.0
        }
    };
    Some(CoinEstimate {
        published_min_per_kill: range.min,
        published_max_per_kill: range.max,
        expected_per_kill: expected,
        expected_per_hour: expected * kills_per_hour,
    })
}

fn drop_estimate(
    drop: &MobDrop,
    item_by_slug: &BTreeMap<&str, &Item>,
    kills_per_hour: f64,
    price_model: PriceModel,
    recently_sold_threshold_multiplier: f64,
) -> DropEstimate {
    let item_slug = drop.item.links.iter().find_map(|link| {
        link.href
            .split('?')
            .next()
            .and_then(|path| path.strip_prefix("/items/"))
            .filter(|slug| !slug.is_empty())
            .map(str::to_string)
    });
    let expected_rolls = drop
        .rolls
        .iter()
        .try_fold(0.0, |sum, roll| Some(sum + roll.chance_percent? / 100.0));
    let expected_quantity = expected_rolls;
    let selected_valuation = item_slug
        .as_deref()
        .and_then(|slug| item_by_slug.get(slug).copied())
        .and_then(|item| match price_model {
            PriceModel::None => None,
            PriceModel::Shop => item
                .shop_price
                .map(|price| (price, DropValuationSource::Shop)),
            PriceModel::MarketObservation => item
                .market_price
                .map(|price| (price, DropValuationSource::MarketObservation)),
            PriceModel::RecentlySoldObservation => item
                .recently_sold_price
                .map(|price| (price, DropValuationSource::RecentlySoldObservation)),
            PriceModel::ShopOrRecentlySold => match (item.shop_price, item.recently_sold_price) {
                (Some(shop), Some(recent))
                    if recent as f64 >= shop as f64 * recently_sold_threshold_multiplier =>
                {
                    Some((recent, DropValuationSource::RecentlySoldObservation))
                }
                (Some(shop), _) => Some((shop, DropValuationSource::Shop)),
                (None, Some(recent)) => {
                    Some((recent, DropValuationSource::RecentlySoldObservation))
                }
                (None, None) => None,
            },
        });
    let selected_price = selected_valuation.map(|(price, _)| price);
    let value_per_kill = expected_quantity
        .zip(selected_price)
        .map(|(quantity, price)| quantity * price as f64);

    DropEstimate {
        item_label: drop.item.text.clone(),
        item_slug,
        independent_rolls: drop.rolls.len(),
        expected_successful_rolls_per_kill: expected_rolls,
        published_chance_at_least_one_percent: drop.solo_chance_at_least_one_percent,
        published_maximum_quantity: drop.maximum_quantity,
        expected_quantity_per_kill: expected_quantity,
        expected_quantity_per_hour: expected_quantity.map(|quantity| quantity * kills_per_hour),
        selected_unit_price: selected_price,
        valuation_source: selected_valuation.map(|(_, source)| source),
        expected_value_per_kill: value_per_kill,
        expected_value_per_hour: value_per_kill.map(|value| value * kills_per_hour),
    }
}

fn nonnegative_finite(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be finite and nonnegative"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ItemRequirements, ItemStats};
    use crate::encounter::{EnergyAssumptions, SimultaneousEventOrder};
    use crate::mobs::{MobDropRoll, MobLink, MobRange, MobRawPage, MobValue};

    fn value(text: &str, href: Option<&str>) -> MobValue {
        MobValue {
            text: text.into(),
            links: href
                .map(|href| {
                    vec![MobLink {
                        href: href.into(),
                        text: text.into(),
                        aria_label: None,
                    }]
                })
                .unwrap_or_default(),
            html: String::new(),
        }
    }

    fn fixture() -> (Mob, Vec<Item>) {
        let drop = MobDrop {
            item: value("Loot", Some("/items/loot-1")),
            rolls: vec![
                MobDropRoll {
                    source: Some("Spawn".into()),
                    chance_percent: Some(50.0),
                    raw: value("", None),
                },
                MobDropRoll {
                    source: Some("Global".into()),
                    chance_percent: Some(25.0),
                    raw: value("", None),
                },
            ],
            solo_chance_at_least_one_percent: Some(62.5),
            maximum_quantity: Some(3),
            raw_cells: Vec::new(),
        };
        let mob = Mob {
            slug: "mob".into(),
            name: "Mob".into(),
            source_url: String::new(),
            level: Some(1),
            health: Some(100),
            damage: Some(MobRange { min: 1, max: 1 }),
            attack_speed_ms: Some(2_000),
            attacks: Some(true),
            faction: None,
            faction_xp: None,
            required_weapon: None,
            aggressive: None,
            debuff_skill_count: None,
            debuffs: Vec::new(),
            locations: Vec::new(),
            drop_profiles: vec![MobDropProfile {
                summary: value("Zone profile", None),
                zone: Some(value("Zone", None)),
                map_location_count: Some(4),
                solo_coins: Some(MobRange { min: 10, max: 20 }),
                drops: vec![drop],
                facts: Vec::new(),
                tables: Vec::new(),
                raw: value("", None),
            }],
            raw: MobRawPage {
                facts: Vec::new(),
                sections: Vec::new(),
                tables: Vec::new(),
                links: Vec::new(),
                source_html: String::new(),
            },
        };
        let item = Item {
            name: "Loot".into(),
            slug: "loot-1".into(),
            item_type: "Junk".into(),
            description: None,
            image_url: None,
            implant_slot: None,
            damage_min: None,
            damage_max: None,
            attack_speed: None,
            stats: ItemStats::default(),
            requirements: ItemRequirements::default(),
            market_price: Some(100),
            recently_sold_price: Some(80),
            shop_price: Some(10),
            sellable_to_shops: true,
            rebirth: false,
            ascension: false,
            unparsed_tooltip_lines: Vec::new(),
        };
        (mob, vec![item])
    }

    fn assumptions() -> GrindingAssumptions {
        GrindingAssumptions {
            encounter: EncounterAssumptions {
                assume_player_survives: false,
                player_level: Some(1),
                player_armor: Some(0),
                simultaneous_event_order: SimultaneousEventOrder::PlayerFirst,
                expected_noncritical_player_hit: 10.0,
                player_crit_percent: 0.0,
                player_attack_interval_seconds: 1.0,
                player_max_health: 100.0,
                player_dodge_percent: 0.0,
                expected_post_mitigation_mob_hit: Some(1.0),
                energy: Some(EnergyAssumptions {
                    initial_energy: 100.0,
                    net_cost_per_second: 0.0,
                }),
            },
            drop_profile_index: 0,
            travel_seconds_per_kill: 1.0,
            recovery_seconds_per_kill: 2.0,
            respawn_wait_seconds_per_kill: 3.0,
            apply_inferred_spawn_cap: false,
            coin_expectation_model: CoinExpectationModel::MidpointOfPublishedRange,
            drop_quantity_model: Some(DropQuantityModel::OnePerSuccessfulRoll),
            price_model: PriceModel::Shop,
            recently_sold_threshold_multiplier: 3.0,
            consumables: Vec::new(),
        }
    }

    #[test]
    fn cycle_rolls_coins_and_shop_value_remain_separate() {
        let (mob, items) = fixture();
        let estimate = estimate_grinding(&mob, &items, assumptions()).unwrap();
        assert_eq!(
            estimate.encounter.expected_event_timeline.elapsed_seconds,
            9.0
        );
        assert_eq!(estimate.cycle_seconds, 15.0);
        assert_eq!(estimate.kills_per_hour, 240.0);
        assert_eq!(estimate.coins.as_ref().unwrap().expected_per_kill, 15.0);
        assert_eq!(
            estimate.drops[0].expected_successful_rolls_per_kill,
            Some(0.75)
        );
        assert_eq!(estimate.drops[0].expected_quantity_per_kill, Some(0.75));
        assert_eq!(estimate.expected_loot_clicks_per_kill, Some(0.75));
        assert_eq!(estimate.expected_loot_clicks_per_hour, Some(180.0));
        assert_eq!(estimate.expected_item_value_per_kill, Some(7.5));
        assert_eq!(
            estimate.expected_total_coins_and_items_per_hour,
            Some(5_400.0)
        );
        assert_eq!(estimate.expected_net_value_per_hour, Some(5_400.0));
    }

    #[test]
    fn faction_xp_per_hour_preserves_metadata_and_nullability() {
        let (mut mob, items) = fixture();
        mob.faction = Some(value("Rangers", None));
        mob.faction_xp = Some(10);
        let estimate = estimate_grinding(&mob, &items, assumptions()).unwrap();
        assert_eq!(estimate.mob_faction_label.as_deref(), Some("Rangers"));
        assert_eq!(estimate.faction_xp_per_kill, Some(10));
        assert_eq!(estimate.expected_faction_xp_per_hour, Some(2_400.0));
        assert_eq!(
            ranking_value(&estimate, GrindingRankingMetric::FactionXpPerHour),
            Some(2_400.0)
        );

        mob.faction_xp = None;
        let estimate = estimate_grinding(&mob, &items, assumptions()).unwrap();
        assert_eq!(estimate.expected_faction_xp_per_hour, None);
    }

    #[test]
    fn consumables_are_subtracted_from_gross_value() {
        let (mob, items) = fixture();
        let mut scenario = assumptions();
        scenario.consumables.push(ConsumableAssumption {
            label: "Potion".into(),
            expected_quantity_per_kill: 0.5,
            unit_cost: 10.0,
        });
        let estimate = estimate_grinding(&mob, &items, scenario).unwrap();
        assert_eq!(estimate.expected_consumable_cost_per_kill, 5.0);
        assert_eq!(estimate.expected_net_value_per_kill, Some(17.5));
        assert_eq!(estimate.expected_net_value_per_hour, Some(4_200.0));
    }

    #[test]
    fn legacy_quantity_policies_are_normalized_to_one_per_successful_roll() {
        let (mob, items) = fixture();
        for legacy_model in [
            None,
            Some(DropQuantityModel::UniformInclusiveOneToPublishedMaximum),
        ] {
            let mut scenario = assumptions();
            scenario.drop_quantity_model = legacy_model;
            let estimate = estimate_grinding(&mob, &items, scenario).unwrap();
            assert_eq!(estimate.drops[0].expected_quantity_per_kill, Some(0.75));
            assert_eq!(estimate.expected_item_value_per_hour, Some(1_800.0));
            assert_eq!(
                estimate.assumptions.drop_quantity_model,
                Some(DropQuantityModel::OnePerSuccessfulRoll)
            );
        }
    }

    #[test]
    fn leaderboard_selects_each_mobs_best_profile_then_sorts_and_limits() {
        let (mut best_mob, items) = fixture();
        best_mob.slug = "best".into();
        best_mob.name = "Best".into();
        let mut better_profile = best_mob.drop_profiles[0].clone();
        better_profile.summary = value("Better profile", None);
        better_profile.solo_coins = Some(MobRange { min: 30, max: 30 });
        best_mob.drop_profiles.push(better_profile);

        let mut second_mob = best_mob.clone();
        second_mob.slug = "second".into();
        second_mob.name = "Second".into();
        second_mob.drop_profiles.truncate(1);
        second_mob.drop_profiles[0].solo_coins = Some(MobRange { min: 20, max: 20 });

        let result = grinding_leaderboard(
            &[second_mob, best_mob],
            &items,
            assumptions(),
            GrindingRankingMetric::ExpectedCoinsPerHour,
            1,
            None,
        )
        .unwrap();

        assert_eq!(result.evaluated_mob_count, 2);
        assert_eq!(result.ranked_mob_count, 2);
        assert_eq!(result.skipped_mob_count, 0);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].rank, 1);
        assert_eq!(result.rows[0].mob_slug, "best");
        assert_eq!(result.rows[0].estimate.drop_profile_index, 1);
        assert_eq!(result.rows[0].ranking_value, 7_200.0);
        assert_eq!(result.rows[0].known_gold_per_hour, Some(9_000.0));
        assert!(
            grinding_leaderboard(
                &[],
                &items,
                assumptions(),
                GrindingRankingMetric::KillsPerHour,
                0,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn leaderboard_can_rank_only_mobs_from_the_selected_faction() {
        let (mut rangers, items) = fixture();
        rangers.slug = "rangers-mob".into();
        rangers.name = "Rangers Mob".into();
        rangers.faction = Some(value("Rangers", None));
        rangers.faction_xp = Some(10);

        let mut pirates = rangers.clone();
        pirates.slug = "pirates-mob".into();
        pirates.name = "Pirates Mob".into();
        pirates.faction = Some(value("Pirates", None));
        pirates.faction_xp = Some(20);

        let result = grinding_leaderboard_for_faction(
            &[pirates, rangers],
            &items,
            assumptions(),
            GrindingRankingMetric::FactionXpPerHour,
            50,
            None,
            Some("rangers"),
        )
        .unwrap();

        assert_eq!(result.faction_filter.as_deref(), Some("rangers"));
        assert_eq!(result.total_mob_count, 2);
        assert_eq!(result.evaluated_mob_count, 1);
        assert_eq!(result.faction_filtered_mob_count, 1);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].mob_slug, "rangers-mob");
    }

    #[test]
    fn net_value_leaderboard_ranks_by_known_gold_when_a_drop_value_is_unknown() {
        let (mut mob, items) = fixture();
        let mut unknown_drop = mob.drop_profiles[0].drops[0].clone();
        unknown_drop.item = value("Unknown loot", Some("/items/unknown-loot"));
        mob.drop_profiles[0].drops.push(unknown_drop.clone());

        let mut scenario = assumptions();
        scenario.consumables.push(ConsumableAssumption {
            label: "Potion".into(),
            expected_quantity_per_kill: 0.5,
            unit_cost: 10.0,
        });
        let result = grinding_leaderboard(
            std::slice::from_ref(&mob),
            &items,
            scenario.clone(),
            GrindingRankingMetric::ExpectedNetValuePerHour,
            10,
            None,
        )
        .unwrap();

        assert_eq!(result.ranked_mob_count, 1);
        assert_eq!(result.rows[0].ranking_value, 4_200.0);
        assert_eq!(result.rows[0].known_gold_per_hour, Some(4_200.0));
        assert_eq!(result.rows[0].estimate.expected_net_value_per_hour, None);
        assert!(
            result
                .notes
                .iter()
                .any(|note| note.contains("unknown drop values contribute zero"))
        );

        mob.drop_profiles[0].solo_coins = None;
        mob.drop_profiles[0].drops = vec![unknown_drop];
        let estimate = estimate_grinding(&mob, &items, scenario).unwrap();
        assert_eq!(known_gold_per_hour(&estimate), None);
    }

    #[test]
    fn hybrid_pricing_uses_recently_sold_only_at_the_configured_premium() {
        let (mob, mut items) = fixture();
        let mut scenario = assumptions();
        scenario.price_model = PriceModel::ShopOrRecentlySold;

        items[0].recently_sold_price = Some(29);
        let drop = &estimate_grinding(&mob, &items, scenario.clone())
            .unwrap()
            .drops[0];
        assert_eq!(drop.selected_unit_price, Some(10));
        assert_eq!(drop.valuation_source, Some(DropValuationSource::Shop));

        items[0].recently_sold_price = Some(30);
        let drop = &estimate_grinding(&mob, &items, scenario.clone())
            .unwrap()
            .drops[0];
        assert_eq!(drop.selected_unit_price, Some(30));
        assert_eq!(
            drop.valuation_source,
            Some(DropValuationSource::RecentlySoldObservation)
        );
        items[0].recently_sold_price = Some(31);
        assert_eq!(
            estimate_grinding(&mob, &items, scenario.clone())
                .unwrap()
                .drops[0]
                .selected_unit_price,
            Some(31)
        );

        items[0].shop_price = None;
        assert_eq!(
            estimate_grinding(&mob, &items, scenario.clone())
                .unwrap()
                .drops[0]
                .selected_unit_price,
            Some(31)
        );
        items[0].recently_sold_price = None;
        let drop = &estimate_grinding(&mob, &items, scenario).unwrap().drops[0];
        assert_eq!(drop.selected_unit_price, None);
        assert_eq!(drop.valuation_source, None);
    }

    #[test]
    fn unknown_roll_probability_propagates_to_loot_clicks() {
        let (mut mob, items) = fixture();
        mob.drop_profiles[0].drops[0].rolls[1].chance_percent = None;
        let estimate = estimate_grinding(&mob, &items, assumptions()).unwrap();
        assert_eq!(estimate.expected_loot_clicks_per_kill, None);
        assert_eq!(estimate.expected_loot_clicks_per_hour, None);
    }

    #[test]
    fn leaderboard_excludes_resources_boss_candidates_and_click_heavy_routes() {
        let (normal, items) = fixture();
        let mut resource = normal.clone();
        resource.slug = "resource".into();
        resource.required_weapon = Some(value("Mining tool", None));
        let mut boss = normal.clone();
        boss.slug = "boss".into();
        boss.raw.source_html = "achievement-boss-example-icon.png".into();

        let included = grinding_leaderboard(
            &[normal.clone(), resource.clone(), boss.clone()],
            &items,
            assumptions(),
            GrindingRankingMetric::KillsPerHour,
            10,
            Some(180.0),
        )
        .unwrap();
        assert_eq!(included.rows.len(), 1);
        assert_eq!(included.rows[0].mob_slug, "mob");
        assert_eq!(included.excluded_resource_mob_count, 1);
        assert_eq!(included.excluded_boss_candidate_count, 1);
        assert_eq!(included.click_filtered_route_count, 0);

        let filtered = grinding_leaderboard(
            &[normal, resource, boss],
            &items,
            assumptions(),
            GrindingRankingMetric::KillsPerHour,
            10,
            Some(179.0),
        )
        .unwrap();
        assert!(filtered.rows.is_empty());
        assert_eq!(filtered.click_filtered_route_count, 1);
    }

    #[test]
    fn click_limit_excludes_routes_with_unknown_click_expectation() {
        let (mut mob, items) = fixture();
        mob.drop_profiles[0].drops[0].rolls[0].chance_percent = None;
        let result = grinding_leaderboard(
            &[mob],
            &items,
            assumptions(),
            GrindingRankingMetric::KillsPerHour,
            10,
            Some(1_000.0),
        )
        .unwrap();
        assert!(result.rows.is_empty());
        assert_eq!(result.click_filtered_route_count, 1);
    }

    #[test]
    fn pricing_threshold_must_be_finite_and_at_least_one() {
        let (mob, items) = fixture();
        for threshold in [0.99, f64::NAN, f64::INFINITY] {
            let mut scenario = assumptions();
            scenario.recently_sold_threshold_multiplier = threshold;
            assert!(estimate_grinding(&mob, &items, scenario).is_err());
        }
    }

    #[test]
    fn comparisons_rank_known_values_and_preserve_failures() {
        let (mob, items) = fixture();
        let mut slower_assumptions = assumptions();
        slower_assumptions.travel_seconds_per_kill = 16.0;
        let result = compare_grinding(
            &[mob],
            &items,
            GrindingComparison {
                ranking_metric: GrindingRankingMetric::ExpectedNetValuePerHour,
                targets: vec![
                    GrindingTarget {
                        mob_slug: "mob".into(),
                        assumptions: assumptions(),
                    },
                    GrindingTarget {
                        mob_slug: "slower".into(),
                        assumptions: slower_assumptions,
                    },
                ],
            },
        )
        .unwrap();
        assert_eq!(result.entries[0].rank, Some(1));
        assert_eq!(result.entries[0].mob_slug, "mob");
        assert_eq!(result.entries[1].rank, None);
        assert!(result.entries[1].error.is_some());
    }

    #[test]
    fn opening_one_shots_reset_attack_interval_and_use_spawn_throughput() {
        let (mut mob, items) = fixture();
        mob.health = Some(5);
        let mut scenario = assumptions();
        scenario.travel_seconds_per_kill = 0.0;
        scenario.recovery_seconds_per_kill = 0.0;
        scenario.respawn_wait_seconds_per_kill = 0.0;

        let estimate = estimate_grinding(&mob, &items, scenario).unwrap();
        assert_eq!(
            estimate.encounter.expected_event_timeline.elapsed_seconds,
            0.0
        );
        assert_eq!(estimate.cycle_seconds, 0.0);
        assert_eq!(estimate.published_map_location_count, Some(4));
        assert_eq!(estimate.inferred_spawn_cap_kills_per_hour, Some(480.0));
        assert_eq!(estimate.uncapped_kills_per_hour, 480.0);
        assert!(estimate.spawn_cap_applied);
        assert_eq!(estimate.kills_per_hour, 480.0);
    }

    #[test]
    fn inferred_spawn_cap_is_opt_in_for_positive_duration_routes() {
        let (mut mob, items) = fixture();
        mob.health = Some(50);
        let mut scenario = assumptions();
        scenario.travel_seconds_per_kill = 0.0;
        scenario.recovery_seconds_per_kill = 0.0;
        scenario.respawn_wait_seconds_per_kill = 0.0;
        scenario.apply_inferred_spawn_cap = true;

        let estimate = estimate_grinding(&mob, &items, scenario).unwrap();
        assert_eq!(estimate.published_map_location_count, Some(4));
        assert_eq!(estimate.cycle_seconds, 4.0);
        assert_eq!(estimate.uncapped_kills_per_hour, 900.0);
        assert_eq!(estimate.inferred_spawn_cap_kills_per_hour, Some(480.0));
        assert!(estimate.spawn_cap_applied);
        assert_eq!(estimate.kills_per_hour, 480.0);
    }

    #[test]
    fn zero_second_cycle_requires_a_published_spawn_count() {
        let (mut mob, items) = fixture();
        mob.health = Some(5);
        mob.drop_profiles[0].map_location_count = None;
        let mut scenario = assumptions();
        scenario.travel_seconds_per_kill = 0.0;
        scenario.recovery_seconds_per_kill = 0.0;
        scenario.respawn_wait_seconds_per_kill = 0.0;

        let error = estimate_grinding(&mob, &items, scenario).unwrap_err();
        assert!(error.contains("positive published spawn count"));
    }

    #[test]
    fn losing_encounters_and_bad_profile_indices_are_rejected() {
        let (mob, items) = fixture();
        let mut losing = assumptions();
        losing.encounter.player_max_health = 1.0;
        assert!(estimate_grinding(&mob, &items, losing).is_err());
        let mut missing = assumptions();
        missing.drop_profile_index = 1;
        assert!(estimate_grinding(&mob, &items, missing).is_err());
    }
}
