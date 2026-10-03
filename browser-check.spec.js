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

  await page.locator("#tab-grinding").click();
  await page.locator("#grind-form").evaluate(form => form.requestSubmit());
  await expect(page.locator("#grind-result .leaderboard-result, #grind-result .result-error")).toBeVisible({ timeout: 30000 });

  expect(apiRequests).toEqual([]);
  expect(consoleErrors).toEqual([]);
});
