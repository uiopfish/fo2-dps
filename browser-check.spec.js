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

  await page.locator("#tab-explorer").click();
  await expect(page.locator("#entity-list .entity-card").first()).toBeVisible();
  await page.locator("#entity-list .entity-card").first().click();
  await expect(page.locator("#detail-dialog")).toBeVisible();
  await page.locator("[data-close-dialog]").click();

  await page.locator("#tab-build").click();
  await expect(page.locator("#build-live-status")).toContainText(/Build valid|Valid/, { timeout: 30000 });

  await page.locator('[data-effect-role="buff"][data-effect-index="0"]').click();
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

  await page.locator("#tab-grinding").click();
  await page.locator("#grind-form").evaluate(form => form.requestSubmit());
  await expect(page.locator("#grind-result .leaderboard-result, #grind-result .result-error")).toBeVisible({ timeout: 30000 });

  expect(apiRequests).toEqual([]);
  expect(consoleErrors).toEqual([]);
});
