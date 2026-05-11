/* eslint-disable no-console */
/**
 * Multi-company Selenium E2E. Drives the real macOS Tauri binary
 * through the full add-and-view flow for a diversified set of filers
 * (MSFT, COST, JPM, BRK.B). Real SEC ingestion, real screenshots.
 *
 * Run from repo root after `cargo build --manifest-path src-tauri/Cargo.toml`:
 *
 *     node tests/selenium/run-multi.mjs
 */
import { spawn, execSync } from "node:child_process";
import { mkdirSync, writeFileSync, existsSync, rmSync } from "node:fs";
import path from "node:path";
import os from "node:os";
import { setTimeout as wait } from "node:timers/promises";

import { Builder, By, until } from "selenium-webdriver";

const REPO = path.resolve(new URL("../..", import.meta.url).pathname);
const BINARY = path.join(REPO, "src-tauri/target/debug/econ-project");
const SCREEN_DIR = path.join(REPO, "tests/selenium/screenshots-multi");
const APP_DATA = path.join(os.homedir(), "Library/Application Support/com.econproject.app");
const PORT = 4445;
const DRIVER_URL = `http://127.0.0.1:${PORT}`;

const COMPANIES = [
  { ticker: "MSFT",  name: "MICROSOFT CORP", reason: "Different fiscal year (June FYE)" },
  { ticker: "COST",  name: "Costco Wholesale", reason: "53-week fiscal years" },
  { ticker: "JPM",   name: "JPMorgan Chase",  reason: "Bank — concept-map coverage stress" },
  { ticker: "BRK.B", name: "Berkshire Hathaway", reason: "Share-class ticker" },
];

if (!existsSync(BINARY)) {
  console.error(`[fatal] binary not found: ${BINARY}`);
  console.error("Run: cargo build --manifest-path src-tauri/Cargo.toml --features e2e-webdriver");
  process.exit(2);
}
mkdirSync(SCREEN_DIR, { recursive: true });

// Start fresh: wipe any previously persisted DB so this is a true cold-start run.
if (existsSync(APP_DATA)) {
  rmSync(APP_DATA, { recursive: true, force: true });
  console.log(`[setup] wiped ${APP_DATA}`);
}

console.log(`[start] launching ${BINARY}`);
const proc = spawn(BINARY, [], {
  env: { ...process.env, TAURI_WEBDRIVER_PORT: String(PORT), RUST_LOG: "info" },
  stdio: ["ignore", "pipe", "pipe"],
});
proc.stdout.on("data", (b) => process.stderr.write(`[app] ${b}`));
proc.stderr.on("data", (b) => process.stderr.write(`[app] ${b}`));
proc.on("exit", (code) => console.log(`[app exited] ${code}`));

async function waitReady(timeoutMs = 15000) {
  const t0 = Date.now();
  while (Date.now() - t0 < timeoutMs) {
    try {
      const resp = await fetch(`${DRIVER_URL}/status`);
      if (resp.ok && (await resp.json())?.value?.ready) return;
    } catch { /* still booting */ }
    await wait(250);
  }
  throw new Error("WebDriver server did not become ready");
}

const screenshot = async (driver, name) => {
  const png = await driver.takeScreenshot();
  const file = path.join(SCREEN_DIR, `${name}.png`);
  writeFileSync(file, Buffer.from(png, "base64"));
  console.log(`[screenshot] ${file}`);
};

let driver;
let exitCode = 0;
const results = [];
try {
  await waitReady();
  console.log("[ready] WebDriver server up");
  driver = await new Builder()
    .usingServer(DRIVER_URL)
    .withCapabilities({ browserName: "webkit", platformName: "macos" })
    .build();

  await driver.wait(async () => /Saved companies/i.test(await driver.findElement(By.tagName("body")).getText()), 10000);

  for (const c of COMPANIES) {
    console.log(`\n── ${c.ticker} (${c.reason}) ──`);
    const result = { ticker: c.ticker, reason: c.reason };

    // Add ticker.
    try {
      // Make sure we're on the home page.
      const headerLinks = await driver.findElements(By.partialLinkText("Saved companies"));
      if (headerLinks.length > 0) await headerLinks[0].click();
      await wait(300);

      const input = await driver.findElement(By.css('input[placeholder*="Ticker"]'));
      await input.clear();
      await input.sendKeys(c.ticker);
      await driver.findElement(By.xpath('//button[normalize-space(.)="Add"]')).click();

      // Wait up to 180s for the ingestion to complete and the row to appear.
      console.log(`  [ingest] waiting for ${c.ticker} to land in saved list (real SEC fetch)…`);
      const t0 = Date.now();
      await driver.wait(async () => {
        const text = await driver.findElement(By.tagName("body")).getText();
        // Diagnostic: print every 10 s.
        const elapsed = Math.floor((Date.now() - t0) / 1000);
        if (elapsed > 0 && elapsed % 10 === 0) {
          const namePart = c.name.split(" ")[0];
          const hasName = text.toLowerCase().includes(namePart.toLowerCase());
          const hasRemove = /remove/i.test(text);
          const hasError = /not found|failed/i.test(text);
          console.log(`    [${elapsed}s] hasName=${hasName} hasRemove=${hasRemove} hasError=${hasError}`);
        }
        // Match the row by company name (more reliable than ticker which appears in the input).
        const namePart = c.name.split(" ")[0];
        return text.toLowerCase().includes(namePart.toLowerCase()) && /remove/i.test(text);
      }, 180000);
      result.added = true;
      console.log(`  [ingest] ✓ added`);
    } catch (e) {
      result.added = false;
      result.error = String(e);
      console.log(`  [ingest] ✗ failed: ${e}`);
      results.push(result);
      continue;
    }

    // Open dashboard.
    try {
      const link = await driver.findElement(By.xpath(
        `//a[contains(@href, "/c/${c.ticker.replace(".", "")}") or .//*[contains(text(), "${c.ticker}")]]`
      ));
      await link.click();
    } catch {
      // Fallback: just find a link whose text starts with the ticker.
      const links = await driver.findElements(By.css("a"));
      for (const a of links) {
        const t = await a.getText();
        if (t.toUpperCase().includes(c.ticker.toUpperCase())) {
          await a.click();
          break;
        }
      }
    }

    try {
      await driver.wait(async () => {
        const t = await driver.findElement(By.tagName("body")).getText();
        return /Time series/i.test(t);
      }, 15000);
    } catch (e) {
      result.dashboard = false;
      result.error = `dashboard didn't render: ${e}`;
      console.log(`  [dashboard] ✗ ${result.error}`);
      results.push(result);
      continue;
    }
    await wait(800);

    // Capture the dashboard text and screenshot.
    const body = await driver.findElement(By.tagName("body")).getText();
    const dollarValues = (body.match(/\$\d+(\.\d+)?[KMBT]/g) ?? []).slice(0, 8);
    result.dashboard = true;
    result.dollarValues = dollarValues;
    console.log(`  [dashboard] ✓ values: ${JSON.stringify(dollarValues)}`);

    await screenshot(driver, `${c.ticker.replace(".", "_")}-dashboard`);

    // Click the first metric widget → drill page → lineage drawer.
    try {
      const widgets = await driver.findElements(By.css("section.grid button"));
      if (widgets.length === 0) throw new Error("no widgets to drill into");
      await widgets[0].click();
      await driver.wait(until.urlContains("/metric/"), 10000);
      await driver.wait(async () => {
        const t = await driver.findElement(By.tagName("body")).getText();
        return /lineage/i.test(t);
      }, 10000);
      const lineageBtns = await driver.findElements(
        By.xpath('//button[contains(translate(., "LINEAGE", "lineage"), "lineage")]')
      );
      if (lineageBtns.length === 0) throw new Error("no lineage button");
      await lineageBtns[lineageBtns.length - 1].click();
      await driver.wait(async () => {
        const t = await driver.findElement(By.tagName("body")).getText();
        return /Source filing/i.test(t) && /Source XBRL concept/i.test(t);
      }, 10000);
      const drawerText = await driver.findElement(By.tagName("body")).getText();
      // Pull a few lineage facts.
      const accnMatch = drawerText.match(/\d{10}-\d{2}-\d{6}/);
      result.lineageAccession = accnMatch?.[0] ?? null;
      const conceptMatch = drawerText.match(/(us-gaap|dei|ifrs-full)\s+\S+/);
      result.lineageConcept = conceptMatch?.[0] ?? null;
      console.log(`  [lineage] accession=${result.lineageAccession} concept=${result.lineageConcept}`);
      await screenshot(driver, `${c.ticker.replace(".", "_")}-lineage`);

      // Close the drawer with Escape so the next iteration starts clean.
      await driver.actions().sendKeys("").perform(); // Escape
      await wait(200);
    } catch (e) {
      result.lineageError = String(e);
      console.log(`  [lineage] ✗ ${e}`);
    }

    // Diagnostics page.
    try {
      const back = await driver.findElement(By.partialLinkText("Saved companies"));
      await back.click();
      await wait(300);
      const link = await driver.findElement(By.xpath(
        `//a[contains(., "${c.name.split(" ")[0]}") or contains(., "${c.ticker}")]`
      ));
      await link.click();
      const diagXpath = '//a[contains(@href,"/diagnostics")]';
      await driver.wait(until.elementLocated(By.xpath(diagXpath)), 10000);
      await driver.findElement(By.xpath(diagXpath)).click();
      await driver.wait(async () => {
        const t = await driver.findElement(By.tagName("body")).getText();
        return /Ingestion complete/i.test(t);
      }, 10000);
      result.diagnosticsLoaded = true;
      console.log(`  [diagnostics] ✓ ingestion event present`);
    } catch (e) {
      result.diagnosticsLoaded = false;
      result.diagnosticsError = String(e);
      console.log(`  [diagnostics] ✗ ${e}`);
    }

    results.push(result);
  }

  // Summary
  console.log("\n=== Multi-company Selenium E2E summary ===");
  for (const r of results) {
    const flag = r.added && r.dashboard && r.lineageAccession && r.diagnosticsLoaded ? "✓" : "·";
    console.log(`${flag} ${r.ticker.padEnd(6)}  ${r.reason}`);
    console.log(`    added=${r.added}  dashboard=${r.dashboard}  values=${JSON.stringify(r.dollarValues ?? [])}`);
    console.log(`    lineage_accession=${r.lineageAccession ?? "—"}  diagnostics=${r.diagnosticsLoaded ? "ok" : "fail"}`);
    if (r.error) console.log(`    error: ${r.error}`);
  }

  const failures = results.filter(
    (r) => !r.added || !r.dashboard || !r.lineageAccession || !r.diagnosticsLoaded
  );
  if (failures.length > 0) {
    console.log(`\n=== ${failures.length} company / ${results.length} had issues ===`);
    exitCode = 1;
  } else {
    console.log("\n=== All companies passed end-to-end ===");
  }
} catch (e) {
  exitCode = 1;
  console.error("[fatal]", e);
  if (driver) {
    try { await screenshot(driver, "fatal-state"); } catch { /* ignore */ }
  }
} finally {
  if (driver) {
    try { await driver.quit(); } catch { /* ignore */ }
  }
  proc.kill("SIGTERM");
  await wait(500);
  proc.kill("SIGKILL");
  process.exit(exitCode);
}
