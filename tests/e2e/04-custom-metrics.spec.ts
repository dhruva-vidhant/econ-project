import path from "node:path";

import { expect, test } from "@playwright/test";

const PAYPAL_FIXTURE = path.resolve(
  new URL("../../src-tauri/tests/fixtures/paypal_kpis.csv", import.meta.url).pathname,
);

test.describe("Custom metrics & CSV export (V2)", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __shouldSeedAapl: boolean }).__shouldSeedAapl = true;
    });
  });

  test("e2e_import_and_chart — import paypal_kpis.csv and see 4 metric charts render", async ({ page }) => {
    await page.goto("/c/AAPL");
    await expect(page.getByText("user data")).toBeVisible();

    await page.getByRole("button", { name: "Import CSV" }).click();
    await page.locator('input[type="file"]').setInputFiles(PAYPAL_FIXTURE);
    await page.getByRole("button", { name: "Import", exact: true }).click();

    await expect(page.getByText(/Imported: 16/)).toBeVisible();
    await expect(page.getByText(/Rejected: 0/)).toBeVisible();
    await page.getByRole("button", { name: "Done" }).click();

    // 4 distinct custom metrics from the fixture, each with its own chart + label.
    await expect(page.getByText("Total Payment Volume").first()).toBeVisible();
    await expect(page.getByText("Payment Transactions").first()).toBeVisible();
    await expect(page.getByText("Active Accounts").first()).toBeVisible();
    await expect(page.getByText("Transactions per Account").first()).toBeVisible();
    await expect(page.locator("canvas, div[data-testid='echarts-mock']").first()).toBeVisible();

    await page.waitForTimeout(500); // ECharts paint
    await page.screenshot({ path: "tests/e2e/screenshots/12-custom-metrics-import.png", fullPage: true });
  });

  test("e2e_overlay — Revenue + TPV comparison chart shows two series", async ({ page }) => {
    await page.goto("/c/AAPL");

    await page.getByRole("button", { name: "Import CSV" }).click();
    await page.locator('input[type="file"]').setInputFiles(PAYPAL_FIXTURE);
    await page.getByRole("button", { name: "Import", exact: true }).click();
    await expect(page.getByText(/Imported: 16/)).toBeVisible();
    await page.getByRole("button", { name: "Done" }).click();

    // Select Revenue (builtin) and tpv (custom) in the Compare section.
    await page.getByLabel("revenue").check();
    await page.locator("label", { hasText: "tpv" }).locator('input[type="checkbox"]').check();

    await expect(page.getByText("Select metrics above to compare them on one chart.")).toHaveCount(0);
    await page.waitForTimeout(500); // ECharts paint
    await page.screenshot({ path: "tests/e2e/screenshots/13-custom-metrics-overlay.png", fullPage: true });
  });

  test("e2e_export_smoke — Download CSV on a custom metric chart invokes export_csv", async ({ page }) => {
    await page.goto("/c/AAPL");

    await page.getByRole("button", { name: "Import CSV" }).click();
    await page.locator('input[type="file"]').setInputFiles(PAYPAL_FIXTURE);
    await page.getByRole("button", { name: "Import", exact: true }).click();
    await expect(page.getByText(/Imported: 16/)).toBeVisible();
    await page.getByRole("button", { name: "Done" }).click();

    await page.getByRole("button", { name: "Download CSV" }).first().click();
    await expect(page.getByRole("button", { name: "Download CSV" }).first()).toHaveText("Download CSV");
    await expect(page.getByText(/Export failed/)).toHaveCount(0);
  });

  test("e2e_offline — custom-metrics import/chart/export keep working with network disabled", async ({ page }) => {
    await page.goto("/c/AAPL");
    await page.evaluate(() => window.__econMock.setOffline(true));

    // Import is local (no network) — must still succeed while offline.
    await page.getByRole("button", { name: "Import CSV" }).click();
    await page.locator('input[type="file"]').setInputFiles(PAYPAL_FIXTURE);
    await page.getByRole("button", { name: "Import", exact: true }).click();
    await expect(page.getByText(/Imported: 16/)).toBeVisible();
    await page.getByRole("button", { name: "Done" }).click();

    await expect(page.getByText("Total Payment Volume").first()).toBeVisible();

    await page.getByRole("button", { name: "Download CSV" }).first().click();
    await expect(page.getByText(/Export failed/)).toHaveCount(0);

    // Ingesting a *new* company is the one operation that genuinely needs
    // the network — confirm it (and only it) fails while offline. Navigate
    // via the in-app link (not page.goto) so the offline flag — an
    // in-memory module variable in the mock — survives the transition.
    await page.getByRole("link", { name: /Saved companies/i }).click();
    await page.getByPlaceholder(/ticker/i).fill("MSFT");
    await page.getByRole("button", { name: /^add$/i }).click();
    await expect(page.getByText(/network is disabled/i)).toBeVisible();
  });
});
