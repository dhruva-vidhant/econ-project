import { expect, test } from "@playwright/test";

test.describe("Company dashboard", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __shouldSeedAapl: boolean }).__shouldSeedAapl = true;
    });
  });

  test("S2.1 — opens the AAPL dashboard with summary widgets and charts", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: /Apple Inc\./i }).click();

    await expect(page.getByText("AAPL").first()).toBeVisible();
    await expect(page.getByText("Apple Inc.")).toBeVisible();

    // Summary widgets show formatted dollar amounts.
    await expect(page.locator("button", { hasText: /\$[\d.]+[KMBT]/ }).first()).toBeVisible();
    await expect(page.getByText(/Time series/i)).toBeVisible();

    await page.waitForTimeout(500); // ECharts paint
    await page.screenshot({ path: "tests/e2e/screenshots/04-dashboard.png", fullPage: true });
  });

  test("S2.2 — annual / quarterly toggle switches the chart axis", async ({ page }) => {
    await page.goto("/c/AAPL");

    // Default (annual) — confirm an FY label is rendered.
    await page.waitForTimeout(400);
    let html = await page.content();
    expect(html).toMatch(/FY\d{4}/);

    // Switch to quarterly.
    await page.getByRole("button", { name: "quarterly" }).first().click();
    await page.waitForTimeout(500);
    html = await page.content();
    expect(html).toMatch(/Q[1-4]/);
    await page.screenshot({ path: "tests/e2e/screenshots/05-dashboard-quarterly.png", fullPage: true });
  });

  test("S6 — clicking a summary widget opens the metric drill page", async ({ page }) => {
    await page.goto("/c/AAPL");
    await page.locator("section.grid button").first().click();

    await expect(page).toHaveURL(/\/c\/AAPL\/metric\//);
    await expect(page.getByRole("button", { name: /lineage/i }).first()).toBeVisible();
    await page.waitForTimeout(500);
    await page.screenshot({ path: "tests/e2e/screenshots/06-metric-drill.png", fullPage: true });
  });

  test("S6.1 — clicking lineage opens the lineage drawer with filing details", async ({ page }) => {
    await page.goto("/c/AAPL/metric/Revenue");
    await page.getByRole("button", { name: /lineage/i }).last().click();

    const drawer = page.getByRole("complementary", { name: /lineage details/i });
    await expect(drawer).toBeVisible();
    await expect(drawer.getByText("Source filing")).toBeVisible();
    await expect(drawer.getByText("0000320193-24-000123")).toBeVisible();
    await expect(drawer.getByText("RevenueFromContractWithCustomerExcludingAssessedTax")).toBeVisible();
    await page.screenshot({ path: "tests/e2e/screenshots/07-lineage-drawer.png", fullPage: true });

    await page.keyboard.press("Escape");
    await expect(drawer).toHaveCount(0);
  });
});
