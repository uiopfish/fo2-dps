"use strict";

const EXPECTED_WEB_BUILD_ID = "2026-09-28-static-pages-v27";
const STATIC_RUNTIME = document.documentElement.dataset.runtime === "static";
let staticRuntimePromise = null;

const EXAMPLES = {
  build: {
    level: 1,
    progression: "spawn",
    allocated: { stamina: 0, strength: 0, agility: 0, intellect: 0 },
    equipment: [],
    skills: [],
    active_skill_effects: [],
    faction_notoriety: null,
    guild_level: null
  },
  encounter: {
    assume_player_survives: false,
    simultaneous_event_order: "player_first",
    expected_noncritical_player_hit: null,
    player_crit_percent: null,
    player_attack_interval_seconds: null,
    player_max_health: null,
    player_dodge_percent: null,
    expected_post_mitigation_mob_hit: null,
    energy: null
  },
  grind: {
    encounter: {
      assume_player_survives: false,
      simultaneous_event_order: "player_first",
      expected_noncritical_player_hit: null,
      player_crit_percent: null,
      player_attack_interval_seconds: null,
      player_max_health: null,
      player_dodge_percent: null,
      expected_post_mitigation_mob_hit: null,
      energy: null
    },
    drop_profile_index: 0,
    travel_seconds_per_kill: 4,
    recovery_seconds_per_kill: 2,
    respawn_wait_seconds_per_kill: 0,
    apply_inferred_spawn_cap: false,
    coin_expectation_model: "midpoint_of_published_range",
    drop_quantity_model: "one_per_successful_roll",
    price_model: "shop",
    recently_sold_threshold_multiplier: 3,
    consumables: []
  }
};
EXAMPLES.compare = {
  ranking_metric: "expected_net_value_per_hour",
  targets: [
    { mob_slug: "angry-skele-8", assumptions: EXAMPLES.grind },
    { mob_slug: "alien-crab-101", assumptions: EXAMPLES.grind }
  ]
};

const THEMES = {
  latte: { label: "Latte", color: "#eff1f5" },
  frappe: { label: "Frappé", color: "#303446" },
  macchiato: { label: "Macchiato", color: "#24273a" },
  mocha: { label: "Mocha", color: "#1e1e2e" }
};

const state = {
  activeTab: "overview",
  explorer: { type: "items", search: "", itemType: "", offset: 0, limit: 24, total: 0, records: [], controller: null },
  grindMode: "leaderboard",
  outfit: {},
  picker: null,
  pickerRequestId: 0,
  pickerController: null,
  skillPicker: null,
  skillPickerRequestId: 0,
  skillPickerController: null,
  itemNames: new Map(),
  itemImages: new Map(),
  itemDetails: new Map(),
  loadingItems: new Set(),
  skillDetails: new Map(),
  loadingSkills: new Set(),
  activeSkills: { buffs: Array(5).fill(null), pet: null },
  buildInspection: null,
  tooltipTarget: null,
  tooltipRequestId: 0
};

const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];
const pretty = value => JSON.stringify(value, null, 2);
const escapeHtml = value => String(value ?? "").replace(/[&<>'"]/g, character => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]);
const humanize = value => String(value).replace(/[_-]+/g, " ").replace(/\b\w/g, letter => letter.toUpperCase());
const storageKey = name => `fo2-dps:${name}`;

function readStored(name, fallback) {
  try { return localStorage.getItem(storageKey(name)) || pretty(fallback); }
  catch { return pretty(fallback); }
}

function store(name, value) {
  try { localStorage.setItem(storageKey(name), value); } catch { /* Storage can be disabled. */ }
}

function applyTheme(theme, persist = true) {
  const selected = Object.hasOwn(THEMES, theme) ? theme : "mocha";
  document.documentElement.dataset.theme = selected;
  const themeColor = $('meta[name="theme-color"]');
  if (themeColor) themeColor.content = THEMES[selected].color;
  const selector = $("#theme-select");
  if (selector) selector.value = selected;
  if (persist) store("theme", selected);
}

function initThemeSwitcher() {
  const selector = $("#theme-select");
  if (!selector) return;
  applyTheme(document.documentElement.dataset.theme, false);
  selector.addEventListener("change", () => applyTheme(selector.value));
}

function readGrindingPreferences() {
  try {
    const value = JSON.parse(localStorage.getItem(storageKey("grinding-preferences")) || "{}");
    return value && typeof value === "object" ? value : {};
  } catch { return {}; }
}

function saveGrindingPreferences() {
  const previous = readGrindingPreferences();
  const factionField = $("#grind-leaderboard-faction");
  const preferences = {
    mode: state.grindMode,
    metric: $("#grind-leaderboard-metric")?.value || "expected_net_value_per_hour",
    faction: factionField?.options.length > 1 ? factionField.value : previous.faction || "",
    spawnCap: $("#grind-leaderboard-spawn-cap")?.checked || false,
    maximumLootClicks: $("#grind-leaderboard-max-clicks")?.value || "",
    travel: $("#grind-leaderboard-travel")?.value || "0",
    recovery: $("#grind-leaderboard-recovery")?.value || "0",
    respawn: $("#grind-leaderboard-respawn")?.value || "0"
  };
  store("grinding-preferences", JSON.stringify(preferences));
}

function restoreGrindingPreferences() {
  const preferences = readGrindingPreferences();
  const values = [
    ["#grind-leaderboard-metric", preferences.metric, ["expected_net_value_per_hour", "faction_xp_per_hour"]],
    ["#grind-leaderboard-max-clicks", preferences.maximumLootClicks],
    ["#grind-leaderboard-travel", preferences.travel],
    ["#grind-leaderboard-recovery", preferences.recovery],
    ["#grind-leaderboard-respawn", preferences.respawn]
  ];
  for (const [selector, value, allowed] of values) {
    const field = $(selector);
    if (field && value != null && (!allowed || allowed.includes(value))) field.value = value;
  }
  if (typeof preferences.spawnCap === "boolean") $("#grind-leaderboard-spawn-cap").checked = preferences.spawnCap;
  if (["leaderboard", "single", "compare"].includes(preferences.mode)) state.grindMode = preferences.mode;
}

async function loadStaticRuntime() {
  if (!staticRuntimePromise) {
    staticRuntimePromise = Promise.all([
      import("./pkg/fo2_dps.js"),
      fetch("./data/app-data.v1.json")
    ]).then(async ([wasm, bundleResponse]) => {
      if (!bundleResponse.ok) throw new Error(`Could not load static game data: ${bundleResponse.status} ${bundleResponse.statusText}`);
      const bundle = await bundleResponse.text();
      await wasm.default();
      wasm.initialize(bundle);
      return wasm;
    });
  }
  return staticRuntimePromise;
}

async function api(path, options = {}) {
  if (STATIC_RUNTIME) {
    if (options.signal?.aborted) throw new DOMException("Request aborted", "AbortError");
    const wasm = await loadStaticRuntime();
    try {
      const body = wasm.request(options.method || "GET", path, options.body || "");
      if (options.signal?.aborted) throw new DOMException("Request aborted", "AbortError");
      return JSON.parse(body);
    } catch (error) {
      if (error instanceof DOMException) throw error;
      throw new Error(typeof error === "string" ? error : error?.message || String(error));
    }
  }

  const response = await fetch(path, { headers: { Accept: "application/json", ...(options.body ? { "Content-Type": "application/json" } : {}) }, ...options });
  const contentType = response.headers.get("content-type") || "";
  let body;
  if (contentType.includes("application/json")) {
    body = await response.json().catch(() => null);
  } else {
    body = await response.text();
  }
  if (!response.ok) {
    const detail = typeof body === "string" ? body : body?.error || body?.message || pretty(body);
    throw new Error(detail || `${response.status} ${response.statusText}`);
  }
  return body;
}

function setTab(name, { focus = false, updateHash = true } = {}) {
  const tab = $(`[data-tab="${name}"]`);
  const panel = $(`[data-panel="${name}"]`);
  if (!tab || !panel) return;
  state.activeTab = name;
  $$("[data-tab]").forEach(button => {
    const selected = button === tab;
    button.setAttribute("aria-selected", selected);
    button.tabIndex = selected ? 0 : -1;
  });
  $$("[data-panel]").forEach(item => { item.hidden = item !== panel; });
  if (focus) tab.focus();
  if (updateHash) history.replaceState(null, "", `#${name}`);
  if (name === "explorer" && !state.explorer.records.length) loadEntities(true);
}

function initTabs() {
  $$("[data-tab]").forEach((tab, index, tabs) => {
    tab.addEventListener("click", () => setTab(tab.dataset.tab));
    tab.addEventListener("keydown", event => {
      if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
      event.preventDefault();
      let target = index;
      if (event.key === "ArrowLeft") target = (index - 1 + tabs.length) % tabs.length;
      if (event.key === "ArrowRight") target = (index + 1) % tabs.length;
      if (event.key === "Home") target = 0;
      if (event.key === "End") target = tabs.length - 1;
      setTab(tabs[target].dataset.tab, { focus: true });
    });
  });
  $$('[data-go]').forEach(button => button.addEventListener("click", () => setTab(button.dataset.go)));
  $$('[data-tab-link]').forEach(link => link.addEventListener("click", event => { event.preventDefault(); setTab(link.dataset.tabLink); }));
  const initial = location.hash.slice(1);
  setTab($(`[data-tab="${initial}"]`) ? initial : "overview", { updateHash: false });
}

function summaryEntries(data) {
  const iconMap = { items: "◆", mobs: "♜", skills: "✦", item_sets: "⬡", itemsets: "⬡", sets: "⬡" };
  const source = data?.counts && typeof data.counts === "object" ? data.counts : data;
  if (!source || typeof source !== "object") return [];
  return Object.entries(source)
    .filter(([, value]) => typeof value === "number" || (value && typeof value.count === "number"))
    .map(([key, value]) => ({ key, value: typeof value === "number" ? value : value.count, icon: iconMap[key.toLowerCase().replace(/-/g, "_")] || "◇" }))
    .slice(0, 8);
}

async function loadSummary() {
  const region = $("#summary-state");
  region.innerHTML = '<div class="stat-card skeleton-card"></div>'.repeat(4);
  const status = $("#api-status");
  status.className = "api-status";
  $(".status-text", status).textContent = "Checking archive…";
  try {
    const data = await api("/api/summary");
    if (data?.web_build_id !== EXPECTED_WEB_BUILD_ID) {
      throw new Error(`Frontend/backend build mismatch: expected ${EXPECTED_WEB_BUILD_ID}, received ${data?.web_build_id || "an older server"}. Stop every existing fo2-dps server and restart it.`);
    }
    const entries = summaryEntries(data);
    const itemTypes = $("#item-types");
    if (itemTypes && Array.isArray(data?.item_types)) {
      itemTypes.innerHTML = data.item_types.map(type => `<option value="${escapeHtml(type)}"></option>`).join("");
    }
    const factionSelect = $("#grind-leaderboard-faction");
    if (factionSelect && Array.isArray(data?.factions)) {
      const selectedFaction = factionSelect.value || readGrindingPreferences().faction || "";
      factionSelect.innerHTML = '<option value="">All factions (gold ranking only)</option>' + data.factions.map(faction => `<option value="${escapeHtml(faction)}">${escapeHtml(faction)}</option>`).join("");
      if (data.factions.includes(selectedFaction)) factionSelect.value = selectedFaction;
    }
    region.innerHTML = entries.length ? entries.map(entry => `
      <article class="stat-card">
        <span class="stat-label">${escapeHtml(humanize(entry.key))}</span>
        <strong class="stat-value">${escapeHtml(entry.value.toLocaleString())}</strong>
        <span class="stat-detail">${entry.icon} Indexed records</span>
      </article>`).join("") : '<div class="empty-state">The API responded, but no numeric summary fields were found.</div>';
    status.classList.add("online");
    $(".status-text", status).textContent = `Archive online · ${data.web_build_id}`;
  } catch (error) {
    region.innerHTML = `<div class="notice error" style="grid-column:1/-1"><strong>Could not load the archive summary.</strong><br>${escapeHtml(error.message)}</div>`;
    status.classList.add("offline");
    $(".status-text", status).textContent = "API unavailable";
  }
}

function entityFacts(record, type) {
  const candidates = type === "items"
    ? [["Type", record.item_type], ["Level", record.requirements?.level ?? record.level], ["Damage", record.damage_min != null ? `${record.damage_min}–${record.damage_max ?? record.damage_min}` : null]]
    : type === "mobs"
      ? [["Level", record.level], ["Health", record.health], ["Attacks", record.attacks == null ? null : record.attacks ? "Yes" : "No"]]
      : type === "skills"
        ? [["Rank", record.rank], ["Level", record.level_requirement], ["Energy", record.energy_cost]]
        : [["Pieces", Array.isArray(record.pieces) ? record.pieces.length : record.pieces], ["Tiers", Array.isArray(record.bonuses) ? record.bonuses.length : record.tiers]];
  return candidates.filter(([, value]) => value !== null && value !== undefined).slice(0, 3);
}

function renderEntities(append = false) {
  const region = $("#entity-list");
  const type = state.explorer.type;
  const cards = state.explorer.records.map(record => {
    const name = record.name || record.label || record.title || record.slug || "Unnamed record";
    const facts = entityFacts(record, type);
    return `<button class="entity-card" type="button" data-slug="${escapeHtml(record.slug || "")}">
      <span class="entity-kind">${escapeHtml(type === "item-sets" ? "Item set" : type.replace(/s$/, ""))}</span>
      <h2>${escapeHtml(name)}</h2>
      <span class="entity-slug">${escapeHtml(record.slug || "No slug")}</span>
      <span class="entity-facts">${facts.map(([label, value]) => `<span class="pill">${escapeHtml(label)} · ${escapeHtml(value)}</span>`).join("")}</span>
    </button>`;
  }).join("");
  region.innerHTML = cards || '<div class="empty-state"><strong>No records found.</strong><br>Try a broader search or another entity type.</div>';
  $$(".entity-card", region).forEach(card => card.addEventListener("click", () => openDetail(card.dataset.slug)));
  $("#explorer-count").textContent = `${state.explorer.total.toLocaleString()} ${state.explorer.total === 1 ? "record" : "records"}`;
  const more = $("#load-more");
  more.hidden = state.explorer.records.length >= state.explorer.total;
  more.disabled = false;
  more.textContent = "Load more records";
}

async function loadEntities(reset = false) {
  const explorer = state.explorer;
  if (reset) { explorer.offset = 0; explorer.records = []; }
  explorer.controller?.abort();
  explorer.controller = new AbortController();
  const region = $("#entity-list");
  const errorBox = $("#explorer-error");
  errorBox.hidden = true;
  region.setAttribute("aria-busy", "true");
  if (reset) region.innerHTML = '<div class="loading-card"></div>'.repeat(6);
  else { $("#load-more").disabled = true; $("#load-more").textContent = "Loading…"; }
  const params = new URLSearchParams({ search: explorer.search, limit: explorer.limit, offset: explorer.offset });
  if (explorer.type === "items" && explorer.itemType) params.set("type", explorer.itemType);
  try {
    const data = await api(`/api/${explorer.type}?${params}`, { signal: explorer.controller.signal });
    const records = Array.isArray(data?.records) ? data.records : [];
    explorer.records = reset ? records : explorer.records.concat(records);
    explorer.total = Number(data?.total ?? explorer.records.length);
    explorer.offset = Number(data?.offset ?? explorer.offset) + Number(data?.limit ?? explorer.limit);
    renderEntities(!reset);
  } catch (error) {
    if (error.name === "AbortError") return;
    if (reset) region.innerHTML = "";
    errorBox.textContent = `Could not load records: ${error.message}`;
    errorBox.hidden = false;
    $("#load-more").disabled = false;
  } finally { region.setAttribute("aria-busy", "false"); }
}

function debounce(callback, delay = 350) {
  let timer;
  return (...args) => { clearTimeout(timer); timer = setTimeout(() => callback(...args), delay); };
}

function initExplorer() {
  const search = $("#entity-search");
  const itemType = $("#item-type");
  const delayedLoad = debounce(() => loadEntities(true));
  search.addEventListener("input", () => { state.explorer.search = search.value.trim(); delayedLoad(); });
  itemType.addEventListener("input", () => { state.explorer.itemType = itemType.value.trim(); delayedLoad(); });
  $("#entity-type").addEventListener("change", event => {
    state.explorer.type = event.target.value;
    $("#item-type-field").hidden = state.explorer.type !== "items";
    loadEntities(true);
  });
  $("#load-more").addEventListener("click", () => loadEntities(false));
}

function objectFacts(data) {
  if (!data || typeof data !== "object") return [];
  return Object.entries(data).filter(([, value]) => ["string", "number", "boolean"].includes(typeof value)).slice(0, 9);
}

async function openDetail(slug, explicitType = state.explorer.type) {
  if (!slug) return;
  const dialog = $("#detail-dialog");
  const type = explicitType;
  $("#detail-kind").textContent = humanize(type === "item-sets" ? "Item set" : type.replace(/s$/, ""));
  $("#detail-title").textContent = slug;
  $("#detail-content").innerHTML = '<div class="result-loading"><div><div class="spinner"></div>Opening archive record…</div></div>';
  if (!dialog.open) dialog.showModal();
  try {
    const data = await api(`/api/${type}/${encodeURIComponent(slug)}`);
    $("#detail-title").textContent = data?.name || data?.label || slug;
    const facts = objectFacts(data);
    $("#detail-content").innerHTML = `
      ${facts.length ? `<div class="detail-facts">${facts.map(([key, value]) => `<div class="detail-fact"><span>${escapeHtml(humanize(key))}</span><strong>${escapeHtml(formatValue(value, key))}</strong></div>`).join("")}</div>` : ""}
      <details class="raw-result" open><summary>Full record JSON</summary><pre>${escapeHtml(pretty(data))}</pre></details>`;
  } catch (error) {
    $("#detail-content").innerHTML = errorMarkup(error, "Could not open record");
  }
}

function dropHandling(drop) {
  if (drop.valuation_source === "shop") return { label: "Sold to shop", className: "shop" };
  if (drop.valuation_source === "recently_sold_observation") return { label: "Not sold to shop · Recently sold price", className: "external" };
  if (drop.valuation_source === "market_observation") return { label: "Not sold to shop · Market price", className: "external" };
  return { label: "Not sold · No selected value", className: "unknown" };
}

function openGrindingBreakdown(row) {
  const estimate = row?.estimate || {};
  const dialog = $("#detail-dialog");
  const coins = estimate.coins;
  const drops = estimate.drops || [];
  const consumables = estimate.consumables || [];
  const knownGold = row?.known_gold_per_hour;
  const unknownValues = drops.filter(drop => drop.expected_value_per_hour == null).length;
  const valuationModel = estimate.assumptions?.price_model === "shop_or_recently_sold" ? "Shop or recently sold" : humanize(estimate.assumptions?.price_model || "Unknown");
  const automaticSpawnBound = estimate.cycle_seconds === 0 && estimate.spawn_cap_applied && !estimate.assumptions?.apply_inferred_spawn_cap;
  const spawnCapacity = estimate.inferred_spawn_cap_kills_per_hour == null
    ? "Unknown"
    : `${formatValue(estimate.inferred_spawn_cap_kills_per_hour)} kills/hour${automaticSpawnBound ? " · automatic for zero-second cycle" : estimate.assumptions?.apply_inferred_spawn_cap ? " · enabled" : " · not applied"}`;

  $("#detail-kind").textContent = "Grinding gold breakdown";
  $("#detail-title").textContent = row?.mob_name || estimate.mob_slug || "Grinding route";
  $("#detail-content").innerHTML = `
    <div class="detail-facts">
      <div class="detail-fact"><span>Expected gold / hour</span><strong>${escapeHtml(formatValue(knownGold))}</strong></div>
      <div class="detail-fact"><span>Kills / hour</span><strong>${escapeHtml(formatValue(estimate.kills_per_hour))}</strong></div>
      <div class="detail-fact"><span>Location</span><strong>${escapeHtml(estimate.zone || estimate.profile_summary || "Unknown")}</strong></div>
      <div class="detail-fact"><span>Published map spawns</span><strong>${escapeHtml(formatValue(estimate.published_map_location_count))}</strong></div>
      <div class="detail-fact"><span>Spawn capacity</span><strong>${escapeHtml(spawnCapacity)}</strong></div>
      <div class="detail-fact"><span>Cap affected result</span><strong>${estimate.spawn_cap_applied ? "Yes" : "No"}</strong></div>
    </div>
    <section class="grind-breakdown-section">
      <div class="breakdown-heading"><div><h3>Expected income per hour</h3><p>${escapeHtml(valuationModel)} valuation</p></div>${unknownValues ? `<span class="breakdown-warning">${unknownValues} unvalued ${unknownValues === 1 ? "item" : "items"}</span>` : ""}</div>
      <div class="table-wrap"><table class="stat-table gold-breakdown-table"><thead><tr><th>Source</th><th>Expected / hour</th><th>Unit value</th><th>Handling</th><th>Gold / hour</th></tr></thead><tbody>
        ${coins ? `<tr class="coin-row"><th>Pure coin drops<small>${escapeHtml(formatValue(coins.expected_per_kill))} expected per kill</small></th><td>—</td><td>1 coin</td><td><span class="handling-badge coins">Dropped as coins</span></td><td class="money-cell">${escapeHtml(formatValue(coins.expected_per_hour))}</td></tr>` : '<tr><th>Pure coin drops</th><td colspan="4">No published coin range</td></tr>'}
        ${drops.map(drop => {
          const handling = dropHandling(drop);
          const itemLabel = drop.item_slug ? `<span class="breakdown-item" data-tooltip-kind="item" data-tooltip-slug="${escapeHtml(drop.item_slug)}">${escapeHtml(drop.item_label)}</span>` : escapeHtml(drop.item_label);
          return `<tr><th>${itemLabel}<small>${escapeHtml(formatValue(drop.expected_quantity_per_kill))} expected per kill</small></th><td>${escapeHtml(formatValue(drop.expected_quantity_per_hour))}</td><td>${drop.selected_unit_price == null ? "Unknown" : escapeHtml(formatValue(drop.selected_unit_price))}</td><td><span class="handling-badge ${handling.className}">${escapeHtml(handling.label)}</span></td><td class="money-cell ${drop.expected_value_per_hour == null ? "unknown-money" : ""}">${drop.expected_value_per_hour == null ? "Unknown" : escapeHtml(formatValue(drop.expected_value_per_hour))}</td></tr>`;
        }).join("") || '<tr><td colspan="5">No item drops in this profile.</td></tr>'}
      </tbody><tfoot><tr><th colspan="4">Known income subtotal</th><td class="money-cell">${escapeHtml(formatValue((coins?.expected_per_hour || 0) + drops.reduce((sum, drop) => sum + (drop.expected_value_per_hour || 0), 0)))}</td></tr>${consumables.length ? `<tr class="cost-row"><th colspan="4">Consumable costs</th><td>−${escapeHtml(formatValue(estimate.expected_consumable_cost_per_hour))}</td></tr>` : ""}<tr class="total-row"><th colspan="4">Expected gold / hour</th><td class="money-cell">${escapeHtml(formatValue(knownGold))}</td></tr></tfoot></table></div>
      ${unknownValues ? '<p class="breakdown-note">Unknown item values contribute zero to the leaderboard’s known gold/hour subtotal. They are not assumed worthless.</p>' : ""}
    </section>
    ${consumables.length ? `<section class="grind-breakdown-section"><div class="breakdown-heading"><div><h3>Consumable costs</h3><p>Subtracted from expected income</p></div></div><ul class="breakdown-cost-list">${consumables.map(item => `<li><span>${escapeHtml(item.label)} · ${escapeHtml(formatValue(item.expected_quantity_per_kill))}/kill</span><strong>−${escapeHtml(formatValue(item.expected_cost_per_hour))} / hour</strong></li>`).join("")}</ul></section>` : ""}`;
  if (!dialog.open) dialog.showModal();
}

function parseJson(textarea, label) {
  try { return JSON.parse(textarea.value); }
  catch (error) { throw new Error(`${label} contains invalid JSON: ${error.message}`); }
}

function validateTextarea(textarea, output) {
  try {
    JSON.parse(textarea.value);
    output.textContent = "✓ Valid JSON";
    output.className = "json-validity valid";
    textarea.removeAttribute("aria-invalid");
    return true;
  } catch (error) {
    output.textContent = `Invalid JSON · ${error.message}`;
    output.className = "json-validity invalid";
    textarea.setAttribute("aria-invalid", "true");
    return false;
  }
}

function formatValue(value, key = "") {
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (value === null) return "—";
  if (typeof value === "number") {
    const formatted = Number.isInteger(value) ? value.toLocaleString() : value.toLocaleString(undefined, { maximumFractionDigits: 2 });
    if (/percent/i.test(key)) return `${formatted}%`;
    if (/seconds/i.test(key)) return `${formatted}s`;
    return formatted;
  }
  return String(value);
}

function flattenMetrics(value, prefix = "", depth = 0, result = []) {
  if (!value || typeof value !== "object" || Array.isArray(value) || depth > 2) return result;
  Object.entries(value).forEach(([key, item]) => {
    const path = prefix ? `${prefix} · ${humanize(key)}` : humanize(key);
    if (["number", "boolean"].includes(typeof item)) result.push({ key, label: path, value: item });
    else if (item && typeof item === "object" && !Array.isArray(item)) flattenMetrics(item, path, depth + 1, result);
  });
  return result;
}

const PRIORITIES = [
  "expected_outgoing_dps", "expected_time_to_kill_seconds", "survives_in_expectation", "expected_remaining_health",
  "kills_per_hour", "cycle_seconds", "expected_net_value_per_hour", "expected_total_coins_and_items_per_hour",
  "level", "allocated_points", "unspent_points", "allocation_budget", "is_valid", "valid", "ranking_value"
];

function selectMetrics(data) {
  const metrics = flattenMetrics(data);
  return metrics.sort((a, b) => {
    const aIndex = PRIORITIES.findIndex(key => a.key === key);
    const bIndex = PRIORITIES.findIndex(key => b.key === key);
    return (aIndex < 0 ? 999 : aIndex) - (bIndex < 0 ? 999 : bIndex);
  }).slice(0, 9);
}

function resultTitle(data, fallback) {
  return data?.mob_slug || data?.name || data?.build?.name || fallback;
}

function renderResult(container, data, title) {
  const metrics = selectMetrics(data);
  const notes = Array.isArray(data?.notes) ? data.notes : Array.isArray(data?.encounter?.notes) ? data.encounter.notes : [];
  const errors = Array.isArray(data?.errors) ? data.errors : [];
  const warnings = Array.isArray(data?.warnings) ? data.warnings : [];
  container.innerHTML = `
    <div class="result-header"><div><p class="eyebrow">Calculation complete</p><h2>${escapeHtml(resultTitle(data, title))}</h2></div></div>
    ${metrics.length ? `<div class="result-metrics">${metrics.map(metric => `<div class="metric"><span class="metric-label">${escapeHtml(metric.label)}</span><strong class="metric-value">${escapeHtml(formatValue(metric.value, metric.key))}</strong></div>`).join("")}</div>` : ""}
    ${errors.length ? `<section class="result-section"><h3>Validation errors</h3><ul class="note-list">${errors.map(note => `<li>${escapeHtml(typeof note === "string" ? note : pretty(note))}</li>`).join("")}</ul></section>` : ""}
    ${warnings.length ? `<section class="result-section"><h3>Warnings</h3><ul class="note-list">${warnings.map(note => `<li>${escapeHtml(typeof note === "string" ? note : pretty(note))}</li>`).join("")}</ul></section>` : ""}
    ${notes.length ? `<section class="result-section"><h3>Model notes</h3><ul class="note-list">${notes.map(note => `<li>${escapeHtml(note)}</li>`).join("")}</ul></section>` : ""}
    <details class="raw-result"><summary>View raw JSON</summary><pre>${escapeHtml(pretty(data))}</pre></details>`;
}

function renderBuildSheet(container, data) {
  const inspection = data.build;
  renderActiveSetSummary(inspection.active_set_tiers || []);
  const build = buildFromGuided();
  const derived = inspection.confirmed_derived_stats || {};
  const itemStats = inspection.item_stats || {};
  const setStats = inspection.set_bonus_stats || {};
  const activeStats = inspection.active_effect_stats || {};
  const activeEnergyRegen = inspection.active_energy_regen || { expected_energy_per_second: 0, effects: [] };
  const finalAttributes = inspection.final_attributes || {};
  const errors = (inspection.issues || []).filter(issue => issue.kind === "error");
  const unresolvedIssues = (inspection.issues || []).filter(issue => issue.kind === "unresolved");
  const damage = derived.panel_damage_min == null ? "Unresolved" : `${formatValue(derived.panel_damage_min)}–${formatValue(derived.panel_damage_max)}`;
  const dodge = derived.displayed_dodge_percent;
  const statCards = [
    ["Maximum health", derived.maximum_health],
    ["Maximum energy", derived.maximum_energy],
    ["Damage", damage],
    ["Expected basic-attack DPS", derived.expected_basic_attack_dps ?? "Unresolved"],
    ["Attack Power", derived.attack_power ?? "Unresolved"],
    ["Armor", derived.armor],
    ["Attack interval", `${formatValue(derived.attack_interval_seconds)}s`],
    ["Critical chance", derived.displayed_crit_percent == null ? "Unresolved" : `${formatValue(derived.displayed_crit_percent)}%`],
    ["Dodge chance", dodge == null ? "Unresolved" : `${formatValue(dodge)}%`],
    ["Out-of-combat HP regen", `${formatValue(derived.out_of_combat_health_regen_per_tick_candidate)} / ${derived.out_of_combat_regen_tick_seconds}s`],
    ["Out-of-combat Energy regen", `${formatValue(derived.out_of_combat_energy_regen_per_tick_candidate)} / ${derived.out_of_combat_regen_tick_seconds}s`],
    ["In-combat Energy regen", `${formatValue(activeEnergyRegen.expected_energy_per_second)} / sec`]
  ];
  const attributes = ["stamina", "strength", "agility", "intellect"];
  const modifiers = [
    ["Attack Power", itemStats.attack_power, setStats.attack_power, activeStats.attack_power], ["Crit", itemStats.crit, setStats.crit, activeStats.crit],
    ["Flat Damage", itemStats.flat_damage, setStats.flat_damage, activeStats.flat_damage], ["Maximum Health", itemStats.max_health, setStats.max_health, activeStats.max_health],
    ["Maximum Energy", itemStats.max_energy, setStats.max_energy, activeStats.max_energy], ["Health Regen", itemStats.health_regen, setStats.health_regen, activeStats.health_regen],
    ["Energy Regen", itemStats.energy_regen, setStats.energy_regen, activeStats.energy_regen], ["Cast Time Reduction", itemStats.cast_time_reduction, setStats.cast_time_reduction, activeStats.cast_time_reduction],
    ["Attack Speed Reduction (ms)", 0, 0, activeStats.attack_speed_reduction_ms]
  ].filter(([, item, set, active]) => item || set || active);
  const attackRecommendations = inspection.recommended_attack_skills || { top: [], model_notes: [] };

  container.innerHTML = `<div class="build-sheet">
    <header class="build-sheet-header"><div><p class="eyebrow">Live character sheet</p><h2>Level ${escapeHtml(inspection.level)} ${escapeHtml(humanize(inspection.progression))}</h2><p>${inspection.allocated_points.toLocaleString()} assigned · ${inspection.unspent_points.toLocaleString()} unassigned</p></div><span class="build-state ${errors.length ? "invalid" : "valid"}">${errors.length ? `${errors.length} invalid` : "Valid build"}</span></header>
    <section class="derived-stat-grid">${statCards.map(([label, value]) => `<article class="derived-stat"><span>${escapeHtml(label)}</span><strong>${escapeHtml(formatValue(value))}</strong></article>`).join("")}</section>
    <section class="sheet-section"><div class="sheet-heading"><h3>Attribute breakdown</h3><span>Base values are 20</span></div><div class="table-wrap"><table class="stat-table"><thead><tr><th>Attribute</th><th>Base</th><th>Allocated</th><th>Gear</th><th>Set</th><th>Buff / pet</th><th>Total</th></tr></thead><tbody>${attributes.map(attribute => `<tr><th>${humanize(attribute)}</th><td>20</td><td>${formatValue(build.allocated?.[attribute] || 0)}</td><td>${formatValue(itemStats[attribute] || 0)}</td><td>${formatValue(setStats[attribute] || 0)}</td><td>${formatValue(activeStats[attribute] || 0)}</td><td><strong>${formatValue(finalAttributes[attribute])}</strong></td></tr>`).join("")}</tbody></table></div></section>

    <section class="sheet-section"><div class="sheet-heading"><h3>Direct modifiers</h3><span>Gear, set, buffs, and pet</span></div>${modifiers.length ? `<div class="modifier-grid">${modifiers.map(([label, item, set, active]) => `<div><span>${escapeHtml(label)}</span><strong>${formatValue((item || 0) + (set || 0) + (active || 0))}</strong><small>Gear ${formatValue(item || 0)} · Set ${formatValue(set || 0)} · Active ${formatValue(active || 0)}</small></div>`).join("")}</div>` : '<p class="sheet-empty">No direct modifiers.</p>'}</section>
    <section class="sheet-section"><div class="sheet-heading"><h3>Top castable attack skills</h3><span>${attackRecommendations.castable_distinct_skills || 0} eligible · ${escapeHtml(formatValue(activeEnergyRegen.expected_energy_per_second))} Energy/s</span></div>${attackRecommendations.top.length ? `<div class="attack-recommendation-grid">${attackRecommendations.top.map((skill, index) => `<article data-tooltip-kind="skill" data-tooltip-slug="${escapeHtml(skill.skill_slug)}"><span class="recommendation-rank">#${index + 1}</span><div><strong>${escapeHtml(skill.skill_name)}${skill.rank == null ? "" : ` · Rank ${escapeHtml(skill.rank)}`}</strong><span>${escapeHtml(formatValue(skill.sustained_expected_dps))} sustained DPS</span><small>${escapeHtml(formatValue(skill.standalone_expected_dps))} burst DPS · ${escapeHtml(formatValue(skill.expected_damage_per_cast))} damage/cast · ${escapeHtml(formatValue(skill.energy_cost))} Energy</small><small>${skill.is_energy_sustainable ? "Fully sustained by active Energy regen" : `${escapeHtml(formatValue(skill.full_energy_casts))} casts from full Energy · needs ${escapeHtml(formatValue(skill.energy_per_second_at_timing_limit))} Energy/s for full speed`}</small></div></article>`).join("")}</div><p class="recommendation-note">Ranked by sustained DPS using active buff, morph, and pet Energy ticks as long-run Energy/second; ties use burst DPS. Out-of-combat regeneration is excluded. Initial Energy is shown as full-energy casts rather than amortized over an invented fight duration.</p>` : '<p class="sheet-empty">No direct-damage skill meets this build’s local level, progression, attribute, and one-cast Energy requirements.</p>'}</section>

    ${(errors.length || unresolvedIssues.length) ? `<section class="sheet-section issues-section"><div class="sheet-heading"><h3>Validation and unresolved rules</h3><span>${errors.length + unresolvedIssues.length} notices</span></div><ul class="note-list">${[...errors, ...unresolvedIssues].map(issue => `<li class="${issue.kind}"><strong>${humanize(issue.kind)}:</strong> ${escapeHtml(issue.message)}</li>`).join("")}</ul></section>` : ""}
    <section class="sheet-section unresolved-section"><div class="sheet-heading"><h3>Mechanics still unresolved</h3><span>Not silently guessed</span></div><ul class="note-list">${(derived.unresolved || []).map(note => `<li>${escapeHtml(note)}</li>`).join("")}</ul></section>
    <details class="raw-result"><summary>View raw JSON</summary><pre>${escapeHtml(pretty(data))}</pre></details>
  </div>`;
}

function loadingMarkup(label) { return `<div class="result-loading"><div><div class="spinner"></div>${escapeHtml(label)}</div></div>`; }
function errorMarkup(error, heading = "Request failed") { return `<div class="result-error" role="alert"><h2>${escapeHtml(heading)}</h2><p>${escapeHtml(error.message || error)}</p></div>`; }

async function runPost({ form, button, result, path, body, loading, title, renderer = renderResult }) {
  button.disabled = true;
  result.innerHTML = loadingMarkup(loading);
  try {
    const data = await api(path, { method: "POST", body: JSON.stringify(body()) });
    renderer(result, data, title);
  } catch (error) {
    result.innerHTML = errorMarkup(error);
  } finally { button.disabled = false; }
}

let buildInspectionTimer;
function scheduleBuildInspection() {
  clearTimeout(buildInspectionTimer);
  $("#build-live-status").textContent = "Checking build…";
  buildInspectionTimer = setTimeout(inspectBuildLive, 250);
}

async function inspectBuildLive() {
  const result = $("#build-result");
  const status = $("#build-live-status");
  try {
    const data = await api("/api/build/inspect", { method: "POST", body: JSON.stringify(buildFromGuided()) });
    const inspection = data.build;
    state.buildInspection = inspection;
    applyBuildStatsToScenarios();
    const errors = (inspection.issues || []).filter(issue => issue.kind === "error");
    const unresolved = (inspection.issues || []).filter(issue => issue.kind === "unresolved");
    updatePointBudget(buildFromGuided(), inspection.final_attributes);
    status.textContent = errors.length ? `${errors.length} validation ${errors.length === 1 ? "error" : "errors"}` : unresolved.length ? `Valid · ${unresolved.length} unresolved` : "Build valid · Live";
    status.parentElement.classList.toggle("invalid", errors.length > 0);
    renderBuildSheet(result, data);
  } catch (error) {
    status.textContent = "Validation unavailable";
    status.parentElement.classList.add("invalid");
    result.innerHTML = errorMarkup(error, "Could not validate build");
  }
}

const EQUIPMENT_SLOTS = [
  ["face", 0, "Face"], ["head", 0, "Head"], ["ring", 0, "Left ring"], ["ring", 1, "Right ring"],
  ["shoulders", 0, "Shoulders"], ["back", 0, "Back"], ["trinket", 0, "Left trinket"], ["trinket", 1, "Right trinket"],
  ["legs", 0, "Legs"], ["chest", 0, "Chest"], ["relic", 0, "Relic"], ["mount", 0, "Mount"],
  ["main-hand", 0, "Main hand"], ["off-hand", 0, "Off hand"], ["guild", 0, "Guild"], ["faction", 0, "Faction"]
];
const IMPLANT_SLOTS = [
  ["implant-brain", 0, "Brain"],
  ["implant-left-arm", 0, "Left arm"], ["implant-heart", 0, "Heart"], ["implant-right-arm", 0, "Right arm"],
  ["implant-left-leg", 0, "Left leg"], ["implant-right-leg", 0, "Right leg"]
];
const OUTFIT_SLOTS = [
  ["outfit-face", 0, "Face"], ["outfit-head", 0, "Head"], ["outfit-shoulders", 0, "Shoulders"], ["outfit-back", 0, "Back"],
  ["outfit-legs", 0, "Legs"], ["outfit-chest", 0, "Chest"], ["outfit-main-hand", 0, "Main hand"], ["outfit-off-hand", 0, "Off hand"],
  ["outfit-mount", 0, "Mount"], ["outfit-fight-line", 0, "Fight line"]
];

function numericValue(selector, fallback = 0) {
  const value = Number($(selector).value);
  return Number.isFinite(value) ? value : fallback;
}

function allocationBudget(level, progression) {
  const base = level * 2;
  if (progression === "spawn") return base;
  const rebirth = base + Math.floor(level / 4);
  return progression === "ascension" ? rebirth + Math.max(0, level - 100) * 20 : rebirth;
}

function updatePointBudget(build, finalAttributes = null) {
  const level = Math.max(1, Math.trunc(Number(build.level) || 1));
  const allocated = build.allocated || {};
  const spent = ["stamina", "strength", "agility", "intellect"].reduce((sum, attribute) => sum + Math.max(0, Math.trunc(Number(allocated[attribute]) || 0)), 0);
  const budget = allocationBudget(level, build.progression || "spawn");
  const unassigned = budget - spent;
  $("#points-budget").textContent = budget.toLocaleString();
  $("#points-spent").textContent = spent.toLocaleString();
  $("#points-unassigned").textContent = unassigned.toLocaleString();
  $("#point-budget").classList.toggle("overspent", unassigned < 0);
  for (const attribute of ["stamina", "strength", "agility", "intellect"]) {
    const fallback = 20 + Math.max(0, Math.trunc(Number(allocated[attribute]) || 0));
    $(`#build-total-${attribute}`).textContent = Number(finalAttributes?.[attribute] ?? fallback).toLocaleString();
  }
}

function migrateBuild(build) {
  if (!build || typeof build !== "object") return build;
  build.equipment = (build.equipment || []).map(entry => {
    if (entry.slot !== "implant") return entry;
    const slug = entry.item_slug || "";
    const bodyPart = slug === "sacred-gauntlet-implant-1492"
      ? "left-arm"
      : ["brain", "heart", "left-arm", "right-arm", "left-leg", "right-leg"].find(part => slug.includes(part));
    return bodyPart ? { ...entry, slot: `implant-${bodyPart}`, slot_index: 0 } : entry;
  });
  return build;
}

function safeJson(textarea, fallback) {
  try { return migrateBuild(JSON.parse(textarea.value)); } catch { return structuredClone(fallback); }
}

function slotKey(slot, index) { return `${slot}:${index}`; }

function slotButton(config, selected, category) {
  const [slot, index, label] = config;
  const slug = selected?.item_slug || selected?.slug || "";
  const name = state.itemNames.get(slug) || selected?.name || slug || "Empty";
  const image = state.itemImages.get(slug) || selected?.image_url;
  return `<button class="equipment-slot slot-${slot} slot-index-${index} ${slug ? "equipped" : ""}" type="button" aria-label="${escapeHtml(`${label}: ${name}`)}" data-slot="${slot}" data-index="${index}" data-label="${escapeHtml(label)}" data-category="${category}" ${slug ? `data-tooltip-kind="item" data-tooltip-slug="${escapeHtml(slug)}"` : ""}>
    <span class="slot-icon" aria-hidden="true">${image ? `<img src="${escapeHtml(image)}" alt="">` : slug ? escapeHtml((name[0] || "◆").toUpperCase()) : "+"}</span>
    <span class="slot-label">${escapeHtml(label)}</span>
    <span class="slot-item">${escapeHtml(name)}</span>
  </button>`;
}

function renderSlotBoard(equipment = []) {
  const byKey = new Map(equipment.map(entry => [slotKey(entry.slot, Number(entry.slot_index || 0)), entry]));
  $("#loadout-equipment").innerHTML = EQUIPMENT_SLOTS.map(config => slotButton(config, byKey.get(slotKey(config[0], config[1])), "equipment")).join("");
  $("#loadout-implants").innerHTML = IMPLANT_SLOTS.map(config => slotButton(config, byKey.get(slotKey(config[0], config[1])), "implants")).join("");
  $("#loadout-outfit").innerHTML = OUTFIT_SLOTS.map(config => slotButton(config, state.outfit[slotKey(config[0], config[1])], "outfit")).join("");
  $("#equipment-count").textContent = `${equipment.filter(entry => !entry.slot.startsWith("implant-")).length}/16`;
  $("#implant-count").textContent = `${equipment.filter(entry => entry.slot.startsWith("implant-")).length}/6`;
  $("#outfit-count").textContent = `${Object.keys(state.outfit).length}/10`;
  $$('[data-slot][data-category]').forEach(button => button.addEventListener("click", () => openItemPicker({
    slot: button.dataset.slot, index: Number(button.dataset.index), label: button.dataset.label, category: button.dataset.category
  })));
  equipment.filter(entry => entry.item_slug && !state.itemNames.has(entry.item_slug)).forEach(entry => loadEquippedName(entry.item_slug));
}

async function loadEquippedName(slug) {
  if (state.loadingItems.has(slug)) return;
  state.loadingItems.add(slug);
  try {
    const item = await api(`/api/items/${encodeURIComponent(slug)}`);
    state.itemNames.set(slug, item.name);
    if (item.image_url) state.itemImages.set(slug, item.image_url);
    state.itemDetails.set(slug, item);
    renderSlotBoard(buildFromGuided(false).equipment);
  } catch { /* Validation will surface missing item slugs. */ }
  finally { state.loadingItems.delete(slug); }
}

function activeSkillSlotButton(role, index, slug, disabled = false) {
  const detail = slug ? state.skillDetails.get(slug) : null;
  const name = detail?.name || slug || (role === "pet" ? "No active pet" : "Empty buff slot");
  const rank = detail?.rank == null ? (slug ? "Rank unknown" : "Choose skill") : `Rank ${detail.rank}`;
  const image = detail?.image_url;
  const label = role === "pet" ? "Pet" : `Buff ${index + 1}`;
  return `<button class="equipment-slot effect-slot ${slug ? "equipped" : ""}" type="button" ${disabled ? "disabled" : ""} aria-label="${escapeHtml(`${label}: ${name} · ${rank}`)}" data-effect-role="${role}" data-effect-index="${index}" ${slug ? `data-tooltip-kind="skill" data-tooltip-slug="${escapeHtml(slug)}"` : ""}>
    <span class="slot-icon" aria-hidden="true">${image ? `<img src="${escapeHtml(image)}" alt="">` : slug ? escapeHtml((name[0] || "◆").toUpperCase()) : "+"}</span>
    <span class="slot-label">${escapeHtml(label)}</span>
    <span class="slot-item">${escapeHtml(name)}</span>
    <span class="slot-rank">${escapeHtml(disabled ? "Unlocks at Rebirth" : rank)}</span>
  </button>`;
}

function renderActiveSetSummary(sets = []) {
  const region = $("#active-set-summary");
  if (!region) return;
  region.innerHTML = sets.length ? sets.map(set => {
    const name = set.set_name || humanize(String(set.set_slug || "Set").replace(/-\d+$/, ""));
    const effects = (set.effects || []).map(effect => `<span><strong>${escapeHtml(effect.value)}</strong>${escapeHtml(effect.stat)}</span>`).join("");
    return `<article class="active-set-card"><div class="active-set-identity"><span>Active Set</span><strong>${escapeHtml(name)}</strong><em>${formatValue(set.equipped_pieces)} pieces</em></div><div class="active-set-tier"><small>${formatValue(set.required_pieces)}-piece bonus active</small><div class="active-set-effects">${effects}</div></div></article>`;
  }).join("") : '<p class="active-set-empty">No active set bonus from the current equipment.</p>';
}

function renderActiveSkillBoard() {
  const progression = $("#build-progression").value;
  $("#buff-slots").innerHTML = state.activeSkills.buffs.map((slug, index) => activeSkillSlotButton("buff", index, slug)).join("");
  $("#pet-slot").innerHTML = activeSkillSlotButton("pet", 0, state.activeSkills.pet, progression === "spawn");
  $("#buff-count").textContent = `${state.activeSkills.buffs.filter(Boolean).length}/5 active`;
  $("#pet-availability").textContent = progression === "spawn" ? "Unlocks at Rebirth" : state.activeSkills.pet ? "1/1 active" : "Available · 0/1 active";
  $$('[data-effect-role]').forEach(button => button.addEventListener("click", () => openSkillPicker({
    kind: "skill", role: button.dataset.effectRole, index: Number(button.dataset.effectIndex), label: button.dataset.effectRole === "pet" ? "Active pet" : `Active buff ${Number(button.dataset.effectIndex) + 1}`
  })));
  [...state.activeSkills.buffs, state.activeSkills.pet].filter(Boolean).forEach(loadActiveSkillDetail);
}

async function loadActiveSkillDetail(slug) {
  if (state.skillDetails.has(slug) || state.loadingSkills.has(slug)) return;
  state.loadingSkills.add(slug);
  try {
    const skill = await api(`/api/skills/${encodeURIComponent(slug)}`);
    state.skillDetails.set(slug, skill);
    renderActiveSkillBoard();
  } catch { /* Build validation reports unavailable skills. */ }
  finally { state.loadingSkills.delete(slug); }
}


function applyBuildToGuided(build) {
  build = migrateBuild(build);
  $("#build-level").value = build.level ?? 1;
  $("#build-progression").value = build.progression || "spawn";
  for (const attribute of ["stamina", "strength", "agility", "intellect"]) $(`#build-${attribute}`).value = build.allocated?.[attribute] ?? 0;
  const ascended = build.progression === "ascension";
  $("#implants-tab").hidden = !ascended;
  if (!ascended && $("[data-loadout-tab=implants]")?.getAttribute("aria-selected") === "true") setLoadoutTab("equipment");
  updatePointBudget(build);
  renderSlotBoard(build.equipment || []);
  const activeEffects = build.active_skill_effects || [];
  const buffs = [...new Set(activeEffects
    .filter(effect => effect.role === "buff" || effect.role === "morph")
    .map(effect => effect.skill_slug))].slice(0, 5);
  state.activeSkills = { buffs: [...buffs, ...Array(5 - buffs.length).fill(null)], pet: activeEffects.find(effect => effect.role === "pet")?.skill_slug || null };
  renderActiveSkillBoard();
}

function buildFromGuided() {
  const previous = safeJson($("#build-json"), EXAMPLES.build);
  const equipment = Array.isArray(previous.equipment) ? previous.equipment : [];
  const activeEffects = [
    ...state.activeSkills.buffs.filter(Boolean).map(skill_slug => ({ skill_slug, role: "buff" })),
    ...(state.activeSkills.pet ? [{ skill_slug: state.activeSkills.pet, role: "pet" }] : [])
  ];
  const skills = [...new Set([...state.activeSkills.buffs, state.activeSkills.pet].filter(Boolean))];
  return {
    ...previous,
    level: Math.max(1, Math.trunc(numericValue("#build-level", 1))),
    progression: $("#build-progression").value,
    allocated: Object.fromEntries(["stamina", "strength", "agility", "intellect"].map(attribute => [attribute, Math.max(0, Math.trunc(numericValue(`#build-${attribute}`)))])),
    equipment,
    skills,
    active_skill_effects: activeEffects,
    faction_notoriety: previous.faction_notoriety ?? null,
    guild_level: previous.guild_level ?? null
  };
}

function syncBuildFromGuided() {
  const textarea = $("#build-json");
  const build = buildFromGuided();
  textarea.value = pretty(build);
  store("build", textarea.value);
  validateTextarea(textarea, $("#build-validity"));
  updatePointBudget(build);
  scheduleBuildInspection();
}

function setBuildDerivedField(selector, sourceSelector, value) {
  const input = $(selector);
  const source = sourceSelector ? $(sourceSelector) : null;
  if (value == null) {
    if (input.dataset.buildDerived === "true") input.value = "";
    input.readOnly = false;
    delete input.dataset.buildDerived;
    if (source) source.textContent = "Required assumption";
    return false;
  }
  input.value = value;
  input.readOnly = true;
  input.dataset.buildDerived = "true";
  if (source) source.textContent = "Build Lab";
  return true;
}

function applyBuildStatsToAssumptions(assumptions) {
  const inspection = state.buildInspection;
  if (!inspection) return assumptions;
  if ((inspection.issues || []).some(issue => issue.kind === "error")) return assumptions;
  const derived = inspection.confirmed_derived_stats || {};
  return {
    ...assumptions,
    expected_noncritical_player_hit: derived.expected_noncritical_basic_attack_hit ?? assumptions.expected_noncritical_player_hit,
    player_attack_interval_seconds: derived.attack_interval_seconds,
    player_max_health: derived.maximum_health,
    ...(derived.displayed_crit_percent == null ? {} : { player_crit_percent: derived.displayed_crit_percent }),
    ...(derived.displayed_dodge_percent == null ? {} : { player_dodge_percent: derived.displayed_dodge_percent })
  };
}

function applyBuildStatsToScenarios() {
  const inspection = state.buildInspection;
  if (!inspection) return;
  const hasValidationErrors = (inspection.issues || []).some(issue => issue.kind === "error");
  const cannotDerive = hasValidationErrors;
  const derived = inspection.confirmed_derived_stats || {};
  const hit = setBuildDerivedField("#enc-hit", "#enc-hit-source", cannotDerive ? null : derived.expected_noncritical_basic_attack_hit);
  const health = setBuildDerivedField("#enc-health", "#enc-health-source", cannotDerive ? null : derived.maximum_health);
  const speed = setBuildDerivedField("#enc-speed", "#enc-speed-source", cannotDerive ? null : derived.attack_interval_seconds);
  const crit = setBuildDerivedField("#enc-crit", "#enc-crit-source", cannotDerive ? null : derived.displayed_crit_percent);
  const dodge = setBuildDerivedField("#enc-dodge", "#enc-dodge-source", cannotDerive ? null : derived.displayed_dodge_percent);
  const inherited = [hit && "expected basic hit", health && "maximum Health", speed && "attack interval", crit && "Crit", dodge && "Dodge", "level and Armor"].filter(Boolean);
  const required = [!hit && "expected noncritical hit", !crit && "Crit", !dodge && "Dodge", hasValidationErrors && "a valid Build Lab configuration"].filter(Boolean);
  const incomingPolicy = $("#enc-assume-survival").checked
    ? "Incoming damage skipped."
    : "Incoming damage derived from mob damage, level, and Armor.";
  $("#scenario-build-sync").innerHTML = `<strong>Using current Build Lab:</strong> ${escapeHtml(inherited.join(", ") || "no combat values can be safely derived")}. <strong>Enter:</strong> ${escapeHtml(required.join(", ") || "no additional combat values")}. ${escapeHtml(incomingPolicy)}`;
  syncEncounterFromGuided();
  syncGrindFromGuided();
}

function applyEncounterToGuided(value) {
  $("#enc-assume-survival").checked = value.assume_player_survives === true;
  $("#enc-order").value = value.simultaneous_event_order || "player_first";
  $("#enc-hit").value = value.expected_noncritical_player_hit ?? "";
  $("#enc-speed").value = value.player_attack_interval_seconds ?? "";
  $("#enc-crit").value = value.player_crit_percent ?? "";
  $("#enc-dodge").value = value.player_dodge_percent ?? "";
  $("#enc-health").value = value.player_max_health ?? "";
  $("#enc-mob-hit").value = value.expected_post_mitigation_mob_hit ?? "";
  $("#enc-energy").value = value.energy?.initial_energy ?? "";
  $("#enc-energy-rate").value = value.energy?.net_cost_per_second ?? "";
}

function encounterFromGuided() {
  const initialEnergy = $("#enc-energy").value.trim();
  return applyBuildStatsToAssumptions({
    assume_player_survives: $("#enc-assume-survival").checked,
    simultaneous_event_order: $("#enc-order").value,
    expected_noncritical_player_hit: numericValue("#enc-hit"),
    player_crit_percent: numericValue("#enc-crit"),
    player_attack_interval_seconds: numericValue("#enc-speed"),
    player_max_health: numericValue("#enc-health"),
    player_dodge_percent: numericValue("#enc-dodge"),
    expected_post_mitigation_mob_hit: $("#enc-mob-hit").value.trim() === "" ? null : numericValue("#enc-mob-hit"),
    energy: initialEnergy === "" ? null : { initial_energy: Number(initialEnergy), net_cost_per_second: numericValue("#enc-energy-rate") }
  });
}

function missingScenarioInputs() {
  const fields = [
    ["#enc-hit", "expected noncritical hit", value => value > 0],
    ["#enc-speed", "attack interval", value => value > 0],
    ["#enc-crit", "displayed Crit", value => value >= 0]
  ];
  if (!$("#enc-assume-survival").checked) fields.push(
    ["#enc-dodge", "displayed Dodge", value => value >= 0 && value <= 100],
    ["#enc-health", "maximum Health", value => value > 0]
  );
  return fields.filter(([selector, , valid]) => {
    const raw = $(selector).value.trim();
    return raw === "" || !valid(Number(raw));
  }).map(([, label]) => label);
}

function syncEncounterFromGuided() {
  const textarea = $("#encounter-json");
  textarea.value = pretty(encounterFromGuided());
  store("encounter", textarea.value);
  validateTextarea(textarea, $("#encounter-validity"));
}

function renderConsumableRows(consumables = []) {
  const region = $("#consumable-rows");
  region.innerHTML = consumables.map((entry, index) => `<div class="editor-row consumable-row" data-index="${index}">
    <input class="row-label" value="${escapeHtml(entry.label || "")}" placeholder="Label" aria-label="Consumable label">
    <input class="row-quantity" type="number" min="0" step="any" value="${Number(entry.expected_quantity_per_kill || 0)}" aria-label="Quantity per kill">
    <input class="row-cost" type="number" min="0" step="any" value="${Number(entry.unit_cost || 0)}" aria-label="Unit cost">
    <button class="remove-row" type="button" aria-label="Remove consumable">×</button>
  </div>`).join("") || '<p class="collection-empty">No consumable costs.</p>';
  $$(".consumable-row", region).forEach(row => {
    row.querySelectorAll("input").forEach(input => input.addEventListener("input", syncGrindFromGuided));
    $(".remove-row", row).addEventListener("click", () => { row.remove(); syncGrindFromGuided(); if (!$(".consumable-row", region)) renderConsumableRows([]); });
  });
}

function applyGrindToGuided(value) {
  $("#grind-profile").value = value.drop_profile_index ?? 0;
  $("#grind-travel").value = value.travel_seconds_per_kill ?? 0;
  $("#grind-recovery").value = value.recovery_seconds_per_kill ?? 0;
  $("#grind-respawn").value = value.respawn_wait_seconds_per_kill ?? 0;
  $("#grind-price").value = ["shop", "shop_or_recently_sold"].includes(value.price_model) ? value.price_model : "shop";
  $("#grind-threshold").value = value.recently_sold_threshold_multiplier ?? 3;
  renderConsumableRows(value.consumables || []);
}

function grindFromGuided() {
  return {
    encounter: encounterFromGuided(),
    drop_profile_index: Math.max(0, Math.trunc(numericValue("#grind-profile"))),
    travel_seconds_per_kill: Math.max(0, numericValue("#grind-travel")),
    recovery_seconds_per_kill: Math.max(0, numericValue("#grind-recovery")),
    respawn_wait_seconds_per_kill: Math.max(0, numericValue("#grind-respawn")),
    apply_inferred_spawn_cap: false,
    coin_expectation_model: "midpoint_of_published_range",
    drop_quantity_model: "one_per_successful_roll",
    price_model: $("#grind-price").value,
    recently_sold_threshold_multiplier: Math.max(1, numericValue("#grind-threshold")),
    consumables: $$(".consumable-row").map(row => ({
      label: $(".row-label", row).value.trim() || "Consumable",
      expected_quantity_per_kill: Number($(".row-quantity", row).value) || 0,
      unit_cost: Number($(".row-cost", row).value) || 0
    }))
  };
}

function syncGrindFromGuided() {
  const textarea = $("#grind-json");
  textarea.value = pretty(grindFromGuided());
  store("grind", textarea.value);
  validateTextarea(textarea, $("#grind-validity"));
}

function leaderboardAssumptions() {
  const assumptions = grindFromGuided();
  return {
    ...assumptions,
    encounter: {
      ...assumptions.encounter,
      assume_player_survives: true,
      expected_post_mitigation_mob_hit: null
    },
    drop_profile_index: 0,
    travel_seconds_per_kill: Math.max(0, numericValue("#grind-leaderboard-travel")),
    recovery_seconds_per_kill: Math.max(0, numericValue("#grind-leaderboard-recovery")),
    respawn_wait_seconds_per_kill: Math.max(0, numericValue("#grind-leaderboard-respawn")),
    apply_inferred_spawn_cap: $("#grind-leaderboard-spawn-cap").checked,
    drop_quantity_model: "one_per_successful_roll",
    price_model: "shop",
    recently_sold_threshold_multiplier: 3,
    consumables: []
  };
}

function renderGrindingLeaderboard(container, data) {
  const leaderboard = data.leaderboard;
  const rows = leaderboard?.rows || [];
  const metric = leaderboard?.ranking_metric;
  const faction = leaderboard?.faction_filter;
  const metricLabel = metric === "faction_xp_per_hour" ? "Faction XP/hour" : "Gold/hour";
  const scopeLabel = faction ? `${faction} · ${metricLabel}` : metricLabel;
  const heading = faction ? `Top ${faction} grinding targets` : "Top grinding targets";
  const noRowsReason = `Of ${formatValue(leaderboard?.evaluated_mob_count || 0)} candidate mobs, ${formatValue(leaderboard?.excluded_resource_mob_count || 0)} resource targets and ${formatValue(leaderboard?.excluded_boss_candidate_count || 0)} boss candidates were excluded; ${formatValue(leaderboard?.failed_route_count || 0)} routes failed estimation, ${formatValue(leaderboard?.unrankable_route_count || 0)} lacked the selected metric, and ${formatValue(leaderboard?.click_filtered_route_count || 0)} exceeded the loot-click limit.`;
  container.innerHTML = `<div class="leaderboard-result">
    <div class="result-header"><div><p class="eyebrow">${escapeHtml(scopeLabel)} ranking</p><h2>${escapeHtml(heading)}</h2><p>${formatValue(leaderboard?.ranked_mob_count || 0)} rankable of ${formatValue(leaderboard?.evaluated_mob_count || 0)} ${faction ? `${escapeHtml(faction)} ` : ""}mobs · showing ${formatValue(rows.length)} · ${formatValue(leaderboard?.excluded_resource_mob_count || 0)} resources and ${formatValue(leaderboard?.excluded_boss_candidate_count || 0)} boss candidates excluded</p></div></div>
    ${rows.length ? `<div class="table-wrap"><table class="stat-table grinding-table"><thead><tr><th>#</th><th>Mob</th><th>Level</th><th>Best location</th><th><button type="button" data-rank-metric="expected_net_value_per_hour">Gold/h</button></th><th>Faction</th><th>XP/kill</th><th><button type="button" data-rank-metric="faction_xp_per_hour">Faction XP/h</button></th><th>Kills/h</th><th>Loot clicks/h</th></tr></thead><tbody>${rows.map(row => {
      const estimate = row.estimate || {};
      const goldPerHour = row.known_gold_per_hour;
      return `<tr><td><strong>${formatValue(row.rank)}</strong></td><th><button type="button" class="table-link" data-grind-row="${escapeHtml(row.rank - 1)}">${escapeHtml(row.mob_name)}</button></th><td>${formatValue(row.mob_level)}</td><td>${escapeHtml(estimate.zone || estimate.profile_summary || "—")}</td><td class="money-cell">${formatValue(goldPerHour)}</td><td>${escapeHtml(estimate.mob_faction_label || "—")}</td><td>${formatValue(estimate.faction_xp_per_kill)}</td><td>${formatValue(estimate.expected_faction_xp_per_hour)}</td><td>${formatValue(estimate.kills_per_hour)}${estimate.spawn_cap_applied ? '<small class="spawn-cap-label">Spawn-capped</small>' : ""}</td><td>${formatValue(estimate.expected_loot_clicks_per_hour)}</td></tr>`;
    }).join("")}</tbody></table></div>` : `<div class="result-error"><h2>No rankable mobs</h2><p>${escapeHtml(noRowsReason)}</p></div>`}
    <section class="result-section"><h3>MVP model</h3><ul class="note-list"><li>${faction ? `Only mobs belonging to ${escapeHtml(faction)} are evaluated.` : "No faction filter is applied."} Each mob's drop profiles are evaluated; only its highest-ranked route is shown.</li><li>An opening one-shot resets the attack interval immediately. With zero route overhead, its finite rate is published map spawns × 120 kills/hour.</li><li>Each successful independent roll means one item and one loot click.</li><li>The spawn cap is optional for positive-duration routes and automatic for a zero-second one-shot cycle.</li><li>Mining, unlocking, and wood-cutting targets are excluded. Boss candidates are excluded because their 30-minute respawn is outside sustained grinding.</li><li>Gold/hour uses shop value only. Unknown shop values contribute zero to the ranking subtotal while remaining visibly unknown in the route breakdown.</li></ul></section>
    <details class="raw-result"><summary>View raw leaderboard JSON</summary><pre>${escapeHtml(pretty(data))}</pre></details>
  </div>`;
  $$('[data-grind-row]', container).forEach(button => button.addEventListener("click", () => openGrindingBreakdown(rows[Number(button.dataset.grindRow)])));
  $$('[data-rank-metric]', container).forEach(button => button.addEventListener("click", () => {
    $("#grind-leaderboard-metric").value = button.dataset.rankMetric;
    $("#grind-form").requestSubmit();
  }));
}

function setLoadoutTab(name) {
  $$('[data-loadout-tab]').forEach(button => button.setAttribute("aria-selected", button.dataset.loadoutTab === name));
  $$('[data-loadout-panel]').forEach(panel => { panel.hidden = panel.dataset.loadoutPanel !== name; });
}

function currentPickerSelection() {
  if (!state.picker) return null;
  const key = slotKey(state.picker.slot, state.picker.index);
  if (state.picker.category === "outfit") return state.outfit[key] || null;
  return buildFromGuided().equipment.find(entry => slotKey(entry.slot, Number(entry.slot_index || 0)) === key) || null;
}

async function openItemPicker(context) {
  state.pickerController?.abort();
  state.picker = { ...context, kind: "item" };
  $("#picker-kind").textContent = "Choose an item";
  $("#item-picker-title").textContent = context.label;
  $("#picker-search-label").textContent = "Search this slot";
  $("#picker-search").placeholder = "Search compatible items…";
  $("#picker-search").value = "";
  $("#picker-count").textContent = "";
  const dialog = $("#item-picker");
  if (!dialog.open) dialog.showModal();
  await loadPickerItems();
  $("#picker-search").focus();
}

async function openSkillPicker(context) {
  state.skillPickerController?.abort();
  state.skillPicker = context;
  $("#skill-picker-kind").textContent = context.role === "pet" ? "Choose a pet" : "Choose a buff";
  $("#skill-picker-title").textContent = context.label;
  $("#skill-picker-search-label").textContent = `Search ${context.role} skills`;
  $("#skill-picker-search").placeholder = `Search ${context.role} skills by name or rank…`;
  $("#skill-picker-search").value = "";
  $("#skill-picker-count").textContent = "";
  const dialog = $("#skill-picker");
  if (!dialog.open) dialog.showModal();
  await loadPickerSkills();
  $("#skill-picker-search").focus();
}

const ITEM_TOOLTIP_STATS = [
  ["armor", "Armor"], ["stamina", "Stamina"], ["strength", "Strength"], ["agility", "Agility"], ["intellect", "Intellect"],
  ["attack_power", "Attack Power"], ["crit", "Crit"], ["damage", "Damage"], ["cast_time_reduction", "Cast Time Reduction"],
  ["max_health", "Maximum Health"], ["max_energy", "Maximum Energy"], ["health_regen", "Health Regen"], ["energy_regen", "Energy Regen"]
];

function signedValue(value) {
  const number = Number(value);
  return `${number > 0 ? "+" : ""}${formatValue(number)}`;
}

function tooltipShell(image, title, subtitle, content) {
  return `<div class="game-tooltip-heading">${image ? `<img src="${escapeHtml(image)}" alt="">` : ""}<div><strong>${escapeHtml(title)}</strong>${subtitle ? `<span>${escapeHtml(subtitle)}</span>` : ""}</div></div>${content}`;
}

function itemTooltipMarkup(item) {
  const combatLines = [];
  if (item.damage_min != null) combatLines.push(`${formatValue(item.damage_min)}–${formatValue(item.damage_max ?? item.damage_min)} Damage`);
  if (item.attack_speed != null) combatLines.push(`${formatValue(item.attack_speed)} Attack Speed`);
  for (const [key, label] of ITEM_TOOLTIP_STATS) if (item.stats?.[key] != null) combatLines.push(`${signedValue(item.stats[key])} ${label}`);
  const requirements = [];
  if (item.ascension) requirements.push("Ascension");
  else if (item.rebirth) requirements.push("Rebirth");
  if (item.requirements?.level != null) requirements.push(`Level ${formatValue(item.requirements.level)}`);
  for (const key of ["stamina", "strength", "agility", "intellect", "faction_notoriety", "guild_level"]) {
    if (item.requirements?.[key] != null) requirements.push(`${humanize(key)} ${formatValue(item.requirements[key])}`);
  }
  const sets = (item.item_sets || []).map(set => `<div class="game-tooltip-set"><strong>${escapeHtml(set.name)} Set Bonus</strong>${(set.bonuses || []).map(bonus => `<span>Bonus at ${formatValue(bonus.required_pieces)} Pieces: ${escapeHtml((bonus.effects || []).map(effect => `${effect.stat} ${effect.value}`).join(" · "))}</span>`).join("")}</div>`).join("");
  const prices = [
    item.market_price == null ? "" : `<span>Market median price: <strong>${formatValue(item.market_price)} coins</strong></span>`,
    item.recently_sold_price == null ? "" : `<span>Recently sold for: <strong>${formatValue(item.recently_sold_price)} coins</strong></span>`,
    item.sellable_to_shops ? (item.shop_price == null ? "" : `<span>Shop value: <strong>${formatValue(item.shop_price)} coins</strong></span>`) : "<span>NOT SELLABLE TO SHOPS</span>"
  ].filter(Boolean).join("");
  const content = `${item.description ? `<p class="game-tooltip-description">${escapeHtml(item.description)}</p>` : ""}
    ${combatLines.length ? `<div class="game-tooltip-lines">${combatLines.map(line => `<span>${escapeHtml(line)}</span>`).join("")}</div>` : ""}
    ${sets}${prices ? `<div class="game-tooltip-prices">${prices}</div>` : ""}
    ${requirements.length ? `<div class="game-tooltip-requirements">${requirements.map(line => `<span>${escapeHtml(line)}</span>`).join("")}</div>` : ""}`;
  return tooltipShell(item.image_url, item.name || item.slug, item.item_type, content);
}

function skillTooltipMarkup(skill) {
  const facts = [];
  if (skill.level_requirement != null) facts.push(`Level ${formatValue(skill.level_requirement)}`);
  for (const requirement of skill.attribute_requirements || []) facts.push(`${humanize(requirement.attribute)} ${requirement.amount == null ? requirement.value : formatValue(requirement.amount)}`);
  if (skill.cast_time != null) facts.push(`Cast time: ${formatValue(skill.cast_time)}s`);
  if (skill.duration?.Seconds != null) facts.push(`Duration: ${formatValue(skill.duration.Seconds / 60)} minutes`);
  if (skill.energy_cost != null) facts.push(`Energy cost: ${formatValue(skill.energy_cost)}`);
  const effects = (skill.effects || []).map(effect => `<span>${escapeHtml(effect.details || effect.effect_type)}</span>`).join("");
  const content = `${effects ? `<div class="game-tooltip-lines game-tooltip-effects">${effects}</div>` : ""}${facts.length ? `<div class="game-tooltip-requirements">${facts.map(fact => `<span>${escapeHtml(fact)}</span>`).join("")}</div>` : ""}`;
  return tooltipShell(skill.image_url, skill.name || skill.slug, skill.rank == null ? "Skill" : `Rank ${skill.rank}`, content);
}

function setGameTooltipVisible(visible) {
  const tooltip = $("#game-tooltip");
  if (typeof tooltip.showPopover === "function") {
    const open = tooltip.matches(":popover-open");
    if (visible && !open) tooltip.showPopover();
    else if (!visible && open) tooltip.hidePopover();
  } else {
    tooltip.hidden = !visible;
  }
}

function positionGameTooltip(event, target) {
  const tooltip = $("#game-tooltip");
  const targetRect = target.getBoundingClientRect();
  const xOrigin = event?.clientX || targetRect.right;
  const yOrigin = event?.clientY || targetRect.top;
  const gap = 14;
  const bounds = tooltip.getBoundingClientRect();
  let left = xOrigin + gap;
  let top = yOrigin + gap;
  if (left + bounds.width > window.innerWidth - 8) left = xOrigin - bounds.width - gap;
  if (top + bounds.height > window.innerHeight - 8) top = window.innerHeight - bounds.height - 8;
  tooltip.style.left = `${Math.max(8, left)}px`;
  tooltip.style.top = `${Math.max(8, top)}px`;
}

function hideGameTooltip() {
  state.tooltipTarget = null;
  state.tooltipRequestId += 1;
  setGameTooltipVisible(false);
}

async function showGameTooltip(target, event) {
  const kind = target.dataset.tooltipKind;
  const slug = target.dataset.tooltipSlug;
  if (!kind || !slug) return;
  state.tooltipTarget = target;
  const requestId = ++state.tooltipRequestId;
  const tooltip = $("#game-tooltip");
  tooltip.innerHTML = '<div class="game-tooltip-loading">Loading details…</div>';
  setGameTooltipVisible(true);
  positionGameTooltip(event, target);
  try {
    const cache = kind === "item" ? state.itemDetails : state.skillDetails;
    let detail = cache.get(slug);
    const incomplete = !detail || (kind === "item" ? !Array.isArray(detail.item_sets) : !Array.isArray(detail.effects));
    if (incomplete) {
      detail = await api(`/api/${kind === "item" ? "items" : "skills"}/${encodeURIComponent(slug)}`);
      cache.set(slug, detail);
      if (kind === "item") {
        state.itemNames.set(slug, detail.name);
        if (detail.image_url) state.itemImages.set(slug, detail.image_url);
      }
    }
    if (requestId !== state.tooltipRequestId || state.tooltipTarget !== target) return;
    tooltip.innerHTML = kind === "item" ? itemTooltipMarkup(detail) : skillTooltipMarkup(detail);
    positionGameTooltip(event, target);
  } catch (error) {
    if (requestId !== state.tooltipRequestId || state.tooltipTarget !== target) return;
    tooltip.innerHTML = `<div class="game-tooltip-loading">${escapeHtml(error.message || error)}</div>`;
    positionGameTooltip(event, target);
  }
}

function initGameTooltips() {
  document.addEventListener("pointerover", event => {
    const target = event.target.closest?.("[data-tooltip-kind][data-tooltip-slug]");
    if (!target || target === state.tooltipTarget) return;
    showGameTooltip(target, event);
  });
  document.addEventListener("pointermove", event => {
    if (state.tooltipTarget) positionGameTooltip(event, state.tooltipTarget);
  });
  document.addEventListener("pointerout", event => {
    const from = event.target.closest?.("[data-tooltip-kind][data-tooltip-slug]");
    const to = event.relatedTarget?.closest?.("[data-tooltip-kind][data-tooltip-slug]");
    if (from && from !== to) hideGameTooltip();
  });
  document.addEventListener("focusin", event => {
    const target = event.target.closest?.("[data-tooltip-kind][data-tooltip-slug]");
    if (target) showGameTooltip(target, null);
  });
  document.addEventListener("focusout", event => {
    if (event.target.closest?.("[data-tooltip-kind][data-tooltip-slug]")) hideGameTooltip();
  });
}

function requirementText(item) {
  const parts = [];
  const requirements = item.requirements || {};
  if (requirements.level) parts.push(`Level ${requirements.level}`);
  for (const attribute of ["stamina", "strength", "agility", "intellect"]) if (requirements[attribute]) parts.push(`${humanize(attribute)} ${requirements[attribute]}`);
  return parts.length ? `Requires ${parts.join(" · ")}` : "No attribute requirement";
}

async function loadPickerItems() {
  const region = $("#picker-results");
  const context = state.picker;
  if (!context || context.kind !== "item") return;
  const requestId = ++state.pickerRequestId;
  state.pickerController?.abort();
  const controller = new AbortController();
  state.pickerController = controller;
  const timeout = setTimeout(() => controller.abort(), 10_000);
  region.innerHTML = loadingMarkup(`Searching ${context.label.toLowerCase()} items…`);
  const params = new URLSearchParams({ slot: context.slot, search: $("#picker-search").value.trim(), limit: 100 });
  try {
    const data = await api(`/api/items?${params}`, { signal: controller.signal });
    if (requestId !== state.pickerRequestId || state.picker !== context) return;
    $("#picker-count").textContent = `${Number(data.total || 0).toLocaleString()} matching items`;
    const selected = currentPickerSelection();
    const unequip = selected ? `<button class="picker-unequip" type="button" data-unequip>Unequip current item</button>` : "";
    region.innerHTML = unequip + (data.records || []).map(item => `<article class="picker-item ${selected?.item_slug === item.slug || selected?.slug === item.slug ? "selected" : ""}">
      <button class="picker-select" type="button" data-pick="${escapeHtml(item.slug)}" data-tooltip-kind="item" data-tooltip-slug="${escapeHtml(item.slug)}">
        <span class="picker-icon" aria-hidden="true">${item.image_url ? `<img src="${escapeHtml(item.image_url)}" alt="">` : escapeHtml((item.name?.[0] || "◆").toUpperCase())}</span>
        <span class="picker-copy"><strong>${escapeHtml(item.name)}</strong>${item.description ? `<small class="picker-description">${escapeHtml(item.description)}</small>` : ""}<small>${escapeHtml(item.item_type)}</small><small>${escapeHtml(requirementText(item))}</small></span>
      </button>
      <button class="picker-details" type="button" data-picker-details="${escapeHtml(item.slug)}">Details</button>
    </article>`).join("") || '<div class="empty-state">No compatible items match this search.</div>';
    $$('[data-pick]', region).forEach(button => button.addEventListener("click", () => selectPickerItem((data.records || []).find(item => item.slug === button.dataset.pick))));
    $$('[data-picker-details]', region).forEach(button => button.addEventListener("click", () => openDetail(button.dataset.pickerDetails, "items")));
    $('[data-unequip]', region)?.addEventListener("click", () => selectPickerItem(null));
  } catch (error) {
    if (requestId === state.pickerRequestId && state.picker === context) {
      const message = error.name === "AbortError" ? "The item search timed out. Try again." : error;
      region.innerHTML = errorMarkup(message, "Could not load compatible items");
    }
  } finally {
    clearTimeout(timeout);
    if (state.pickerController === controller) state.pickerController = null;
  }
}

async function loadPickerSkills() {
  const region = $("#skill-picker-results");
  const context = state.skillPicker;
  if (!context) return;
  const requestId = ++state.skillPickerRequestId;
  state.skillPickerController?.abort();
  const controller = new AbortController();
  state.skillPickerController = controller;
  const timeout = setTimeout(() => controller.abort(), 10_000);
  region.innerHTML = loadingMarkup(`Searching ${context.role} skills…`);
  const params = new URLSearchParams({ role: context.role, search: $("#skill-picker-search").value.trim(), limit: 100 });
  try {
    const data = await api(`/api/skills?${params}`, { signal: controller.signal });
    if (requestId !== state.skillPickerRequestId || state.skillPicker !== context) return;
    $("#skill-picker-count").textContent = `${Number(data.total || 0).toLocaleString()} matching ${context.role}s`;
    const selectedSlug = context.role === "pet" ? state.activeSkills.pet : state.activeSkills.buffs[context.index];
    const selected = selectedSlug ? { skill_slug: selectedSlug, ...(state.skillDetails.get(selectedSlug) || {}) } : null;
    const activeSlugs = new Set([...state.activeSkills.buffs, state.activeSkills.pet].filter(Boolean));
    const unequip = selected ? `<button class="picker-unequip" type="button" data-unequip>Clear current ${context.role}</button>` : "";
    region.innerHTML = unequip + (data.records || []).map(skill => {
      const usedElsewhere = activeSlugs.has(skill.slug) && selected?.skill_slug !== skill.slug;
      const rank = skill.rank == null ? "Rank unknown" : `Rank ${skill.rank}`;
      return `<article class="picker-item ${selected?.skill_slug === skill.slug ? "selected" : ""}" data-tooltip-kind="skill" data-tooltip-slug="${escapeHtml(skill.slug)}">
        <button class="picker-select" type="button" data-pick-skill="${escapeHtml(skill.slug)}" data-tooltip-kind="skill" data-tooltip-slug="${escapeHtml(skill.slug)}" ${usedElsewhere ? "disabled" : ""}>
          <span class="picker-icon" aria-hidden="true">${skill.image_url ? `<img src="${escapeHtml(skill.image_url)}" alt="">` : escapeHtml((skill.name?.[0] || "◆").toUpperCase())}</span>
          <span class="picker-copy"><strong>${escapeHtml(skill.name)} <span class="picker-rank">${escapeHtml(rank)}</span></strong><small>${escapeHtml((skill.effect_types || []).join(" · ") || humanize(context.role))}</small><small>${skill.level_requirement ? `Requires level ${escapeHtml(skill.level_requirement)}` : "No level requirement"}</small>${usedElsewhere ? "<small>Already active in another slot</small>" : ""}</span>
        </button>
        <button class="picker-details" type="button" data-picker-skill-details="${escapeHtml(skill.slug)}">Details</button>
      </article>`;
    }).join("") || `<div class="empty-state">No ${escapeHtml(context.role)} skills match this search.</div>`;
    $$('[data-pick-skill]', region).forEach(button => button.addEventListener("click", () => selectPickerSkill((data.records || []).find(skill => skill.slug === button.dataset.pickSkill))));
    $$('[data-picker-skill-details]', region).forEach(button => button.addEventListener("click", () => openDetail(button.dataset.pickerSkillDetails, "skills")));
    $('[data-unequip]', region)?.addEventListener("click", () => selectPickerSkill(null));
  } catch (error) {
    if (requestId === state.skillPickerRequestId && state.skillPicker === context) {
      const message = error.name === "AbortError" ? `The ${context.role} search timed out. Try again.` : error;
      region.innerHTML = errorMarkup(message, `Could not load ${context.role} skills`);
    }
  } finally {
    clearTimeout(timeout);
    if (state.skillPickerController === controller) state.skillPickerController = null;
  }
}

function selectPickerSkill(skill) {
  const context = state.skillPicker;
  if (!context) return;
  const slug = skill?.slug || null;
  if (context.role === "pet") state.activeSkills.pet = slug;
  else state.activeSkills.buffs[context.index] = slug;
  if (skill) state.skillDetails.set(skill.slug, skill);
  const build = buildFromGuided();
  $("#build-json").value = pretty(build);
  applyBuildToGuided(build);
  syncBuildFromGuided();
  $("#skill-picker").close();
}

function selectPickerItem(item) {
  const context = state.picker;
  if (!context) return;
  const key = slotKey(context.slot, context.index);
  if (context.category === "outfit") {
    if (item) { state.outfit[key] = item; state.itemNames.set(item.slug, item.name); if (item.image_url) state.itemImages.set(item.slug, item.image_url); } else delete state.outfit[key];
    store("outfit", JSON.stringify(state.outfit));
    renderSlotBoard(buildFromGuided().equipment);
  } else {
    const build = buildFromGuided();
    build.equipment = build.equipment.filter(entry => slotKey(entry.slot, Number(entry.slot_index || 0)) !== key);
    if (item) {
      build.equipment.push({ slot: context.slot, slot_index: context.index, item_slug: item.slug });
      state.itemNames.set(item.slug, item.name);
      if (item.image_url) state.itemImages.set(item.slug, item.image_url);
    }
    $("#build-json").value = pretty(build);
    applyBuildToGuided(build);
    syncBuildFromGuided();
  }
  $("#item-picker").close();
}

async function loadSuggestions(type, search, datalistSelector) {
  if (search.trim().length < 2) return;
  try {
    const data = await api(`/api/${type}?search=${encodeURIComponent(search.trim())}&limit=20`);
    $(datalistSelector).innerHTML = (data.records || []).map(record => `<option value="${escapeHtml(record.slug)}">${escapeHtml(record.name || "")}</option>`).join("");
  } catch { /* Suggestions are optional; form submission still reports exact errors. */ }
}

function initEditors() {
  const fields = {
    build: [$("#build-json"), $("#build-validity")], encounter: [$("#encounter-json"), $("#encounter-validity")],
    grind: [$("#grind-json"), $("#grind-validity")], compare: [$("#compare-json"), $("#grind-validity")]
  };
  try { state.outfit = JSON.parse(localStorage.getItem(storageKey("outfit")) || "{}"); } catch { state.outfit = {}; }
  Object.entries(fields).forEach(([name, [textarea, output]]) => {
    textarea.value = readStored(name, EXAMPLES[name]);
    textarea.addEventListener("input", () => { store(name, textarea.value); validateTextarea(textarea, output); });
  });
  applyBuildToGuided(safeJson(fields.build[0], EXAMPLES.build));
  applyEncounterToGuided(safeJson(fields.encounter[0], EXAMPLES.encounter));
  applyGrindToGuided(safeJson(fields.grind[0], EXAMPLES.grind));
  restoreGrindingPreferences();
  validateTextarea(...fields.build); validateTextarea(...fields.encounter); validateTextarea(...fields.grind);

  ["#build-level", "#build-stamina", "#build-strength", "#build-agility", "#build-intellect"].forEach(selector => $(selector).addEventListener("input", syncBuildFromGuided));
  $("#build-progression").addEventListener("input", () => { syncBuildFromGuided(); applyBuildToGuided(buildFromGuided()); });
  $$('[data-loadout-tab]').forEach(button => button.addEventListener("click", () => setLoadoutTab(button.dataset.loadoutTab)));
  ["#enc-order", "#enc-assume-survival", "#enc-hit", "#enc-speed", "#enc-crit", "#enc-dodge", "#enc-health", "#enc-mob-hit", "#enc-energy", "#enc-energy-rate"].forEach(selector => $(selector).addEventListener("input", () => { syncEncounterFromGuided(); syncGrindFromGuided(); }));
  $("#enc-assume-survival").addEventListener("input", applyBuildStatsToScenarios);
  ["#grind-profile", "#grind-travel", "#grind-recovery", "#grind-respawn", "#grind-price", "#grind-threshold"].forEach(selector => $(selector).addEventListener("input", syncGrindFromGuided));
  ["#grind-leaderboard-metric", "#grind-leaderboard-faction", "#grind-leaderboard-spawn-cap", "#grind-leaderboard-max-clicks", "#grind-leaderboard-travel", "#grind-leaderboard-recovery", "#grind-leaderboard-respawn"].forEach(selector => $(selector).addEventListener("input", saveGrindingPreferences));

  $("#add-consumable").addEventListener("click", () => { const grind = grindFromGuided(); grind.consumables.push({ label: "", expected_quantity_per_kill: 0, unit_cost: 0 }); renderConsumableRows(grind.consumables); });
  $("#download-build").addEventListener("click", () => {
    syncBuildFromGuided();
    const url = URL.createObjectURL(new Blob([fields.build[0].value], { type: "application/json" }));
    const link = document.createElement("a"); link.href = url; link.download = `fo2-build-level-${$("#build-level").value}.json`; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 0); toast("Build JSON downloaded");
  });
  $("#import-build").addEventListener("change", async event => {
    const file = event.target.files?.[0]; if (!file) return;
    try {
      const value = migrateBuild(JSON.parse(await file.text())); fields.build[0].value = pretty(value); applyBuildToGuided(value); syncBuildFromGuided(); toast(`Imported ${file.name}`);
    } catch (error) { $("#build-result").innerHTML = errorMarkup(error, "Could not import build"); }
    event.target.value = "";
  });
  fields.build[0].addEventListener("blur", () => { try { applyBuildToGuided(parseJson(fields.build[0], "Build")); syncBuildFromGuided(); } catch {} });
  fields.encounter[0].addEventListener("blur", () => { try { applyEncounterToGuided(parseJson(fields.encounter[0], "Encounter")); applyBuildStatsToScenarios(); } catch {} });
  fields.grind[0].addEventListener("blur", () => { try { const value = parseJson(fields.grind[0], "Grinding"); applyGrindToGuided(value); if (value.encounter) applyEncounterToGuided(value.encounter); applyBuildStatsToScenarios(); } catch {} });
  ["#encounter-mob", "#grind-mob"].forEach(selector => $(selector).addEventListener("input", debounce(event => loadSuggestions("mobs", event.target.value, "#mob-slugs"), 200)));

  $$('[data-reset]').forEach(button => button.addEventListener("click", () => {
    const name = button.dataset.reset; const [textarea, output] = fields[name];
    textarea.value = pretty(EXAMPLES[name]); store(name, textarea.value); validateTextarea(textarea, output);
    if (name === "build") applyBuildToGuided(EXAMPLES.build);
    if (name === "encounter") { applyEncounterToGuided(EXAMPLES.encounter); applyBuildStatsToScenarios(); }
    if (name === "grind") { applyGrindToGuided(EXAMPLES.grind); applyBuildStatsToScenarios(); }
    textarea.focus(); toast(`${humanize(name)} example restored`);
  }));

  $("#build-form").addEventListener("submit", event => { event.preventDefault(); syncBuildFromGuided(); });
  $("#encounter-form").addEventListener("submit", event => {
    event.preventDefault(); const mob = $("#encounter-mob").value.trim(); const result = $("#encounter-result");
    if (!mob) { result.innerHTML = errorMarkup("Enter a mob slug before running the encounter.", "Mob required"); return; }
    syncBuildFromGuided(); syncEncounterFromGuided();
    const missing = missingScenarioInputs();
    if (missing.length) { result.innerHTML = errorMarkup(`Enter ${missing.join(", ")} from the in-game panel or resolve it in Build Lab.`, "Combat assumptions required"); return; }
    runPost({ button: $('button[type="submit"]', event.currentTarget), result, path: `/api/encounter/${encodeURIComponent(mob)}`, body: () => ({ build: buildFromGuided(), assumptions: encounterFromGuided() }), loading: "Resolving combat timeline…", title: "Encounter result" });
  });
  scheduleBuildInspection();

  $("#grind-form").addEventListener("submit", event => {
    event.preventDefault(); const result = $("#grind-result"); const button = $('button[type="submit"]', event.currentTarget); syncBuildFromGuided();
    if (state.grindMode === "leaderboard") {
      const rankingMetric = $("#grind-leaderboard-metric").value;
      const faction = $("#grind-leaderboard-faction").value;
      saveGrindingPreferences();
      if (rankingMetric === "faction_xp_per_hour" && !faction) { result.innerHTML = errorMarkup("Choose the faction you are grinding before ranking by faction XP.", "Faction required"); return; }
      const missing = missingScenarioInputs().filter(label => !["displayed Dodge", "maximum Health"].includes(label));
      if (missing.length) { result.innerHTML = errorMarkup(`Enter ${missing.join(", ")} from the in-game panel or resolve it in Build Lab.`, "Combat assumptions required"); return; }
      runPost({
        button,
        result,
        path: "/api/grind/leaderboard",
        body: () => ({
          build: buildFromGuided(),
          assumptions: leaderboardAssumptions(),
          ranking_metric: rankingMetric,
          faction: faction || null,
          limit: 50,
          maximum_loot_clicks_per_hour: $("#grind-leaderboard-max-clicks").value.trim() === ""
            ? null
            : Math.max(0, Number($("#grind-leaderboard-max-clicks").value))
        }),
        loading: faction ? `Evaluating ${faction} mobs and drop profiles…` : "Evaluating every mob and drop profile…",
        title: "Top grinding targets",
        renderer: renderGrindingLeaderboard
      });
      return;
    }
    if (state.grindMode === "compare") {
      let comparison; try { comparison = parseJson(fields.compare[0], "Comparison"); } catch (error) { result.innerHTML = errorMarkup(error, "Check the comparison JSON"); return; }
      comparison = { ...comparison, targets: (comparison.targets || []).map(target => ({ ...target, assumptions: { ...target.assumptions, encounter: applyBuildStatsToAssumptions(target.assumptions?.encounter || {}) } })) };
      runPost({ button, result, path: "/api/grind/compare", body: () => ({ build: buildFromGuided(), comparison }), loading: "Ranking farming routes…", title: "Route comparison" }); return;
    }
    const mob = $("#grind-mob").value.trim(); if (!mob) { result.innerHTML = errorMarkup("Enter a mob slug before calculating the route.", "Mob required"); return; }
    const missing = missingScenarioInputs();
    if (missing.length) { result.innerHTML = errorMarkup(`Enter ${missing.join(", ")} from the in-game panel or resolve it in Build Lab.`, "Combat assumptions required"); return; }
    syncGrindFromGuided();
    runPost({ button, result, path: `/api/grind/${encodeURIComponent(mob)}`, body: () => ({ build: buildFromGuided(), assumptions: grindFromGuided() }), loading: "Calculating hourly yield…", title: "Grinding estimate" });
  });
}

function setGrindingMode(mode) {
  state.grindMode = mode;
  saveGrindingPreferences();
  $$('[data-grind-mode]').forEach(item => item.setAttribute("aria-selected", item.dataset.grindMode === mode));
  $("#grind-leaderboard-fields").hidden = mode !== "leaderboard";
  $("#grind-single-fields").hidden = mode !== "single";
  $("#grind-compare-fields").hidden = mode !== "compare";
  $("#grind-layout").classList.toggle("leaderboard-layout", mode === "leaderboard");
  $("#grind-submit-label").textContent = mode === "leaderboard" ? "Rank top 50" : mode === "single" ? "Calculate yield" : "Compare routes";
  if (mode === "leaderboard") {
    $("#grind-validity").textContent = "Uses current Build Lab";
    $("#grind-validity").className = "json-validity valid";
  } else {
    const activeTextarea = mode === "single" ? $("#grind-json") : $("#compare-json");
    validateTextarea(activeTextarea, $("#grind-validity"));
  }
}

function initGrindingMode() {
  $$('[data-grind-mode]').forEach(button => button.addEventListener("click", () => setGrindingMode(button.dataset.grindMode)));
  setGrindingMode(state.grindMode);
}

function toast(message) {
  const item = document.createElement("div");
  item.className = "toast";
  item.textContent = message;
  $("#toast-region").append(item);
  setTimeout(() => item.remove(), 2600);
}

function initDialog() {
  const dialog = $("#detail-dialog");
  $('[data-close-dialog]').addEventListener("click", () => dialog.close());
  dialog.addEventListener("click", event => { if (event.target === dialog) dialog.close(); });
  const picker = $("#item-picker");
  $('[data-close-picker]').addEventListener("click", () => { state.pickerController?.abort(); picker.close(); });
  picker.addEventListener("click", event => { if (event.target === picker) { state.pickerController?.abort(); picker.close(); } });
  $("#picker-search").addEventListener("input", debounce(loadPickerItems, 250));
  const skillPicker = $("#skill-picker");
  $('[data-close-skill-picker]').addEventListener("click", () => { state.skillPickerController?.abort(); skillPicker.close(); });
  skillPicker.addEventListener("click", event => { if (event.target === skillPicker) { state.skillPickerController?.abort(); skillPicker.close(); } });
  $("#skill-picker-search").addEventListener("input", debounce(loadPickerSkills, 250));
}

function init() {
  initThemeSwitcher();
  initTabs();
  initExplorer();
  initEditors();
  initGrindingMode();
  initDialog();
  initGameTooltips();
  $("#refresh-summary").addEventListener("click", loadSummary);
  loadSummary();
}

document.addEventListener("DOMContentLoaded", init);
