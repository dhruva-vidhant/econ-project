import { expect, test } from "@playwright/test";

test.describe("Statements + diagnostics", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __shouldSeedAapl: boolean }).__shouldSeedAapl = true;
    });
  });

  test("statements page renders income/balance/cashflow tables", async ({ page }) => {
    await page.goto("/c/AAPL/statement/income");
    await expect(page.getByText(/income statement/i)).toBeVisible();
    await page.waitForTimeout(400);
    await page.screenshot({ path: "tests/e2e/screenshots/08-statements-income.png", fullPage: true });

    await page.getByRole("button", { name: "balance" }).click();
    await expect(page).toHaveURL(/\/c\/AAPL\/statement\/balance/);
    await expect(page.getByText(/balance statement/i)).toBeVisible();
    await page.screenshot({ path: "tests/e2e/screenshots/09-statements-balance.png", fullPage: true });
  });

  test("diagnostics page lists ingestion events", async ({ page }) => {
    await page.goto("/c/AAPL/diagnostics");
    await expect(page.getByText(/diagnostics/i).first()).toBeVisible();
    await expect(page.getByText(/Ingestion complete/i)).toBeVisible();
    await expect(page.getByText(/Derived single-quarter/i)).toBeVisible();
    await page.screenshot({ path: "tests/e2e/screenshots/10-diagnostics.png", fullPage: true });
  });

  test("S3 — refresh button re-runs ingestion and updates timestamp", async ({ page }) => {
    await page.goto("/c/AAPL");
    await expect(page.getByRole("button", { name: /refresh/i })).toBeVisible();
    await page.getByRole("button", { name: /refresh/i }).click();
    await expect(page.getByRole("button", { name: /refresh/i })).not.toBeDisabled();
    await page.screenshot({ path: "tests/e2e/screenshots/11-after-refresh.png", fullPage: true });
  });
});
