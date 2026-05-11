import { expect, test } from "@playwright/test";

test.describe("Home page", () => {
  test("S1.1 — empty state on first run", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: /saved companies/i })).toBeVisible();
    await expect(page.getByText(/no companies yet/i)).toBeVisible();
    await page.screenshot({ path: "tests/e2e/screenshots/01-home-empty.png", fullPage: true });
  });

  test("S1.2 — adding AAPL ingests and shows the company in the list", async ({ page }) => {
    await page.goto("/");
    await page.getByPlaceholder(/ticker/i).fill("AAPL");
    await page.getByRole("button", { name: /^add$/i }).click();

    await expect(page.getByRole("link", { name: /Apple Inc\./i })).toBeVisible();
    await expect(page.getByText("AAPL", { exact: false }).first()).toBeVisible();
    await page.screenshot({ path: "tests/e2e/screenshots/02-home-with-aapl.png", fullPage: true });
  });

  test("S5 — adding an unknown ticker surfaces a clear error", async ({ page }) => {
    await page.goto("/");
    await page.getByPlaceholder(/ticker/i).fill("XYZNOPE");
    await page.getByRole("button", { name: /^add$/i }).click();

    await expect(page.getByText(/not found in SEC ticker map/i)).toBeVisible();
    await expect(page.getByRole("link", { name: /Apple Inc\./i })).toHaveCount(0);
    await page.screenshot({ path: "tests/e2e/screenshots/03-home-error.png", fullPage: true });
  });

  test("removes a saved company", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __shouldSeedAapl: boolean }).__shouldSeedAapl = true;
    });
    await page.goto("/");
    await expect(page.getByRole("link", { name: /Apple Inc\./i })).toBeVisible();
    await page.getByRole("button", { name: /^remove$/i }).click();
    await expect(page.getByText(/no companies yet/i)).toBeVisible();
  });
});
