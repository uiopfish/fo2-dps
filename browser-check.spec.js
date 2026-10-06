const { test, expect } = require("/home/archfish/.npm/_npx/e41f203b7505f1fb/node_modules/playwright/test");

test.use({ launchOptions: { executablePath: "/usr/bin/chromium" }, viewport: { width: 1440, height: 1000 } });

test("static Pages build runs without the Rust HTTP API", async ({ page }) => {
  const consoleErrors = [];
  const apiRequests = [];
  page.on("console", message => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  page.on("request", request => {
    if (new URL(request.url()).pathname.startsWith("/api/")) apiRequests.push(request.url());
  });

  await page.goto("http://127.0.0.1:4173/", { waitUntil: "networkidle" });
  await expect(page.locator("html")).toHaveAttribute("data-runtime", "static");
  await expect(page.locator("#api-status")).toHaveClass(/online/, { timeout: 30000 });
  await expect(page.locator("#summary-state .stat-card")).toHaveCount(5);
  await expect(page.locator("#tab-encounter")).toHaveCount(0);
  await expect(page.locator("#panel-encounter")).toHaveCount(0);

  await page.locator("#tab-explorer").click();
  await expect(page.locator("#entity-list .entity-card").first()).toBeVisible();
  await page.locator("#entity-list .entity-card").first().click();
  await expect(page.locator("#detail-dialog")).toBeVisible();
  await page.locator("[data-close-dialog]").click();

  await page.locator("#tab-build").click();
  await expect(page.locator("#build-live-status")).toContainText(/Build valid|Valid/, { timeout: 30000 });
  await expect(page.locator("#build-result")).not.toHaveClass(/result-placeholder/);
  const visibleLoadoutTabs = page.locator(".loadout-tabs button:visible");
  const loadoutTabsBox = await page.locator(".loadout-tabs").boundingBox();
  const lastVisibleTabBox = await visibleLoadoutTabs.last().boundingBox();
  expect(Math.abs(lastVisibleTabBox.x + lastVisibleTabBox.width - loadoutTabsBox.x - loadoutTabsBox.width)).toBeLessThan(2);

  const normalSlotHeight = (await page.locator('[data-slot="face"]').boundingBox()).height;
  const normalBoardHeight = (await page.locator("#loadout-equipment").boundingBox()).height;
  const normalFormHeight = (await page.locator("#build-form").boundingBox()).height;
  await page.locator(".yokou-toggle").click();
  await expect(page.locator("#yokou-mode")).toBeChecked();
  await expect(page.locator("html")).toHaveAttribute("data-yokou-mode", "true");
  const compactSlotHeight = (await page.locator('[data-slot="face"]').boundingBox()).height;
  const compactBoardHeight = (await page.locator("#loadout-equipment").boundingBox()).height;
  const compactFormBox = await page.locator("#build-form").boundingBox();
  const compactFormHeight = compactFormBox.height;
  expect(compactSlotHeight).toBeLessThan(normalSlotHeight);
  expect(compactBoardHeight).toBeLessThan(normalBoardHeight * 0.85);
  expect(compactFormHeight).toBeLessThan(normalFormHeight * 0.75);
  expect(compactFormBox.y + compactFormBox.height).toBeLessThanOrEqual(1000);
  const compactResultBox = await page.locator("#build-result").boundingBox();
  expect(compactResultBox.y + compactResultBox.height).toBeLessThanOrEqual(1000);
  const compactEquipmentGrid = await page.locator("#loadout-equipment").evaluate(board => {
    const styles = getComputedStyle(board);
    return {
      columns: styles.gridTemplateColumns.split(" ").length,
      rows: styles.gridTemplateRows.split(" ").length
    };
  });
  expect(compactEquipmentGrid).toEqual({ columns: 4, rows: 4 });
  const buffGroupBox = await page.locator(".active-effect-group").first().boundingBox();
  const petGroupBox = await page.locator(".active-effect-group").nth(1).boundingBox();
  expect(Math.abs(buffGroupBox.y - petGroupBox.y)).toBeLessThan(2);
  await page.reload({ waitUntil: "networkidle" });
  await expect(page.locator("#yokou-mode")).toBeChecked();
  await page.locator("#tab-build").click();
  await page.locator(".yokou-toggle").click();
  await expect(page.locator("#yokou-mode")).not.toBeChecked();

  const relicBox = await page.locator('[data-slot="relic"]').boundingBox();
  const mountBox = await page.locator('[data-slot="mount"]').boundingBox();
  const guildBox = await page.locator('[data-slot="guild"]').boundingBox();
  const factionBox = await page.locator('[data-slot="faction"]').boundingBox();
  expect(relicBox.y).toBeLessThan(guildBox.y);
  expect(mountBox.y).toBeLessThan(factionBox.y);

  const assertRequiredLevelThenName = async locator => {
    const entries = await locator.evaluateAll(rows => rows.map(row => ({
      level: Number(row.dataset.requiredLevel),
      name: row.dataset.displayName
    })));
    expect(entries.length).toBeGreaterThan(1);
    for (let index = 1; index < entries.length; index += 1) {
      const previous = entries[index - 1];
      const current = entries[index];
      expect(current.level > previous.level || (current.level === previous.level && current.name >= previous.name)).toBeTruthy();
    }
  };

  await page.locator('[data-slot="face"]').click();
  await expect(page.locator("#picker-results .picker-item").first()).toBeVisible({ timeout: 30000 });
  await assertRequiredLevelThenName(page.locator("#picker-results .picker-item"));
  await page.locator("[data-close-picker]").click();

  await page.locator('[data-effect-role="buff"][data-effect-index="0"]').click();
  await expect(page.locator("#skill-picker-results .picker-item").first()).toBeVisible({ timeout: 30000 });
  await assertRequiredLevelThenName(page.locator("#skill-picker-results .picker-item"));
  await page.locator("#skill-picker-search").fill("Manhole Manifest");
  await expect(page.locator('[data-pick-skill="manhole-manifest-1st-edition-2848"]')).toBeVisible({ timeout: 30000 });
  await page.locator("[data-close-skill-picker]").click();

  await page.locator("#build-progression").selectOption("spawn");
  await page.locator("#build-level").fill("1");
  for (const attribute of ["stamina", "strength", "agility", "intellect"]) {
    await page.locator(`#build-${attribute}`).fill("0");
  }
  await page.locator("#build-stamina").fill("3");
  await expect(page.locator("#build-stamina")).toHaveValue("2");
  await expect(page.locator("#points-unassigned")).toHaveText("0");
  await expect(page.locator("#build-total-stamina")).toHaveText("22");

  await page.locator("#build-stamina").fill("-1");
  await expect(page.locator("#build-stamina")).toHaveValue("0");
  await expect(page.locator("#build-total-stamina")).toHaveText("20");

  await page.locator("#build-stamina").fill("2");
  await page.locator("#build-strength").fill("1");
  await expect(page.locator("#build-strength")).toHaveValue("0");
  await expect(page.locator("#build-strength")).toHaveAttribute("max", "0");

  await page.locator("#build-level").fill("10");
  await page.locator("#build-stamina").fill("15");
  await page.locator("#build-strength").fill("5");
  await page.locator("#build-level").fill("1");
  await expect(page.locator("#build-stamina")).toHaveValue("2");
  await expect(page.locator("#build-strength")).toHaveValue("0");
  await expect(page.locator("#points-spent")).toHaveText("2");

  await page.locator("#build-form .advanced-editor summary").click();
  await page.locator("#copy-build-code").click();
  const buildCode = await page.locator("#build-code").inputValue();
  const compactPayload = JSON.parse(Buffer.from(buildCode, "base64url").toString("utf8"));
  const verboseV1Code = Buffer.from(JSON.stringify({ v: 1, build: JSON.parse(await page.locator("#build-json").inputValue()) }), "utf8").toString("base64url");
  expect(compactPayload[0]).toBe(2);
  expect(buildCode.length).toBeLessThan(100);
  expect(buildCode.length).toBeLessThan(verboseV1Code.length / 2);
  await page.locator("#build-level").fill("10");
  await page.locator("#build-code").fill(buildCode);
  await page.locator("#load-build-code").click();
  await expect(page.locator("#build-level")).toHaveValue("1");
  await expect(page.locator("#build-stamina")).toHaveValue("2");

  await page.locator("#tab-grinding").click();
  await expect(page.locator("#scenario-build-sync")).toBeVisible();
  await expect(page.locator('#grind-leaderboard-metric option[value="expected_coins_per_hour"]')).toHaveText("Pure coin drops per hour");
  await page.locator("#grind-leaderboard-metric").selectOption("expected_coins_per_hour");
  await page.locator("#grind-form").evaluate(form => form.requestSubmit());
  await expect(page.locator("#grind-result .leaderboard-result, #grind-result .result-error")).toBeVisible({ timeout: 30000 });
  await expect(page.locator("#grind-result .eyebrow")).toContainText("Pure coin drops/hour ranking");
  await expect(page.locator('#grind-result [data-rank-metric="expected_coins_per_hour"]')).toBeVisible();
  await expect(page.locator("#grind-result .result-header")).toContainText("61 daily-dungeon mobs excluded");

  expect(apiRequests).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("legacy v1 shared build URL loads without local setup", async ({ page }) => {
  const build = {
    level: 10,
    progression: "spawn",
    allocated: { stamina: 0, strength: 20, agility: 0, intellect: 0 },
    equipment: [],
    skills: [],
    active_skill_effects: [],
    faction_notoriety: null,
    guild_level: null
  };
  const code = Buffer.from(JSON.stringify({ v: 1, build }), "utf8").toString("base64url");
  await page.goto(`http://127.0.0.1:4173/?build=${code}#build`, { waitUntil: "networkidle" });
  await expect(page.locator("#tab-build")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#build-level")).toHaveValue("10");
  await expect(page.locator("#build-strength")).toHaveValue("20");
  await expect(page.locator("#build-code-status")).toHaveText("Loaded build from shared URL.");
});

test("compact v2 shared URL resolves item and skill IDs", async ({ page }) => {
  const payload = [2, 103, 2, 0, 0, 0, 0, [1, 0, 387], [2848], [0, 2848]];
  const code = Buffer.from(JSON.stringify(payload), "utf8").toString("base64url");
  expect(code.length).toBeLessThan(100);

  await page.goto(`http://127.0.0.1:4173/?build=${code}#build`, { waitUntil: "networkidle" });
  await expect(page.locator("#tab-build")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#build-level")).toHaveValue("103");
  await expect(page.locator('[data-slot="face"]')).toHaveAttribute("data-tooltip-slug", "moon-talisman-387");
  await expect(page.locator('[data-effect-role="buff"][data-effect-index="0"]')).toHaveAttribute("data-tooltip-slug", "manhole-manifest-1st-edition-2848");
  await expect(page.locator("#build-code-status")).toHaveText("Loaded build from shared URL.");
});
