/* eslint-disable no-console */
/**
 * Real W3C-WebDriver end-to-end test against the actual macOS Tauri binary.
 *
 * - Launches the Tauri debug binary with TAURI_WEBDRIVER_PORT=4445.
 * - Connects Selenium WebDriver via the embedded
 *   `tauri-plugin-wdio-webdriver` (no external driver process).
 * - Drives the real React UI in the real WKWebView.
 * - The Rust backend hits real SEC EDGAR for AAPL ingestion.
 *
 * Run from the repo root after the dev binary is built:
 *
 *     cargo build --manifest-path src-tauri/Cargo.toml
 *     node tests/selenium/run-e2e.mjs
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync, existsSync } from "node:fs";
import path from "node:path";
import { setTimeout as wait } from "node:timers/promises";

import { Builder, By, until } from "selenium-webdriver";

const REPO = path.resolve(new URL("../..", import.meta.url).pathname);
const BINARY = path.join(REPO, "src-tauri/target/debug/econ-project");
const SCREEN_DIR = path.join(REPO, "tests/selenium/screenshots");
const PORT = 4445;
const DRIVER_URL = `http://127.0.0.1:${PORT}`;

if (!existsSync(BINARY)) {
  console.error(`[fatal] binary not found: ${BINARY}`);
  console.error("Run: cargo build --manifest-path src-tauri/Cargo.toml --features e2e-webdriver");
  process.exit(2);
}
mkdirSync(SCREEN_DIR, { recursive: true });

console.log(`[start] launching ${BINARY}`);
const proc = spawn(BINARY, [], {
  env: { ...process.env, TAURI_WEBDRIVER_PORT: String(PORT), RUST_LOG: "info" },
  stdio: ["ignore", "pipe", "pipe"],
});
proc.stdout.on("data", (b) => process.stderr.write(`[app stdout] ${b}`));
proc.stderr.on("data", (b) => process.stderr.write(`[app stderr] ${b}`));
proc.on("exit", (code) => console.log(`[app exited] ${code}`));

// Wait for the WebDriver server to become ready.
async function waitReady(timeoutMs = 15000) {
  const t0 = Date.now();
  while (Date.now() - t0 < timeoutMs) {
    try {
      const resp = await fetch(`${DRIVER_URL}/status`);
      if (resp.ok) {
        const j = await resp.json();
        if (j?.value?.ready) return;
      }
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
try {
  await waitReady();
  console.log("[ready] WebDriver server up");

  driver = await new Builder()
    .usingServer(DRIVER_URL)
    .withCapabilities({ browserName: "webkit", platformName: "macos" })
    .build();

  // ── S1: home page renders empty state ────────────────────────────────
  console.log("[S1] home empty state");
  await driver.wait(until.elementLocated(By.css("body")), 10000);
  // The home page heading uses an h3 ("Saved companies"); look for it.
  await driver.wait(async () => {
    const text = await driver.findElement(By.tagName("body")).getText();
    return /Saved companies/i.test(text);
  }, 10000);
  await screenshot(driver, "01-home-empty");

  // ── S2: type AAPL and add (real SEC ingestion) ───────────────────────
  console.log("[S2] add AAPL — real ingestion against live SEC");
  const tickerInput = await driver.findElement(By.css('input[placeholder*="Ticker"]'));
  await tickerInput.sendKeys("AAPL");
  const addBtn = await driver.findElement(By.xpath('//button[normalize-space(.)="Add"]'));
  await addBtn.click();
  console.log("[S2] waiting for Apple Inc. row …");
  await driver.wait(async () => {
    const t = await driver.findElement(By.tagName("body")).getText();
    return /Apple Inc\./.test(t);
  }, 90000);
  console.log("[S2] AAPL added");
  await screenshot(driver, "02-home-with-aapl");

  // ── S3: open the dashboard ───────────────────────────────────────────
  console.log("[S3] open AAPL dashboard");
  const link = await driver.findElement(By.partialLinkText("Apple Inc."));
  await link.click();
  await driver.wait(async () => {
    const t = await driver.findElement(By.tagName("body")).getText();
    // Dashboard widgets show $XB or $XT formatted values.
    return /\$\d+(\.\d+)?[KMBT]/.test(t) && /Time series/i.test(t);
  }, 30000);
  console.log("[S3] dashboard rendered");
  await wait(800); // let ECharts paint
  await screenshot(driver, "03-dashboard");

  // ── S4: verify revenue widget shows a Apple-realistic figure ─────────
  console.log("[S4] verify revenue widget");
  const body4 = await driver.findElement(By.tagName("body")).getText();
  const matches = body4.match(/\$\d+(\.\d+)?B/g) ?? [];
  console.log(`[S4] dollar values found: ${JSON.stringify(matches.slice(0, 8))}`);
  if (matches.length < 1) throw new Error("no dollar-formatted widgets visible");

  // ── S5: open metric drill page + lineage drawer ──────────────────────
  console.log("[S5] click first summary widget → drill page");
  const widgetBtn = await driver.findElement(By.css("section.grid button"));
  await widgetBtn.click();
  await driver.wait(until.urlContains("/metric/"), 10000);
  await driver.wait(async () => {
    const t = await driver.findElement(By.tagName("body")).getText();
    return /lineage/i.test(t);
  }, 10000);
  await wait(500);
  await screenshot(driver, "04-metric-drill");

  console.log("[S5b] click lineage to open drawer");
  const lineageBtns = await driver.findElements(By.xpath('//button[contains(translate(., "LINEAGE", "lineage"), "lineage")]'));
  if (lineageBtns.length === 0) throw new Error("no lineage button found");
  await lineageBtns[lineageBtns.length - 1].click();
  await driver.wait(async () => {
    const t = await driver.findElement(By.tagName("body")).getText();
    return /Source filing/i.test(t) && /Source XBRL concept/i.test(t);
  }, 10000);
  // Verify a real Apple filing accession is shown.
  const drawerText = await driver.findElement(By.tagName("body")).getText();
  if (!/0000320193-/.test(drawerText)) {
    throw new Error("lineage drawer did not surface Apple's actual filing accession");
  }
  console.log("[S5b] lineage drawer shows real Apple filing accession");
  await screenshot(driver, "05-lineage-drawer");

  // ── S6: navigate to statements via in-app link (real router) ────────
  console.log("[S6] click ‘Income’ link → income statement");
  // Walk back to the dashboard via the header crumb, then click Income.
  const back = await driver.findElement(By.partialLinkText("Saved companies"));
  await back.click();
  await wait(300);
  const aapl = await driver.findElement(By.partialLinkText("Apple Inc."));
  await aapl.click();
  // The link text is lowercase 'income' (CSS capitalize only changes display).
  const incomeXpath = '//a[contains(@href,"/statement/income")]';
  await driver.wait(until.elementLocated(By.xpath(incomeXpath)), 10000);
  await driver.findElement(By.xpath(incomeXpath)).click();
  await driver.wait(async () => {
    const t = await driver.findElement(By.tagName("body")).getText();
    return /Income statement/i.test(t);
  }, 10000);
  await wait(500);
  await screenshot(driver, "06-statements-income");

  // ── S7: diagnostics page lists real ingestion events ────────────────
  console.log("[S7] click diagnostics → ingestion events list");
  // The dashboard nav bar has a 'diagnostics' link.
  await driver.findElement(By.partialLinkText("Saved companies")).click();
  await wait(300);
  await driver.findElement(By.partialLinkText("Apple Inc.")).click();
  const diagXpath = '//a[contains(@href,"/diagnostics")]';
  await driver.wait(until.elementLocated(By.xpath(diagXpath)), 10000);
  await driver.findElement(By.xpath(diagXpath)).click();
  await driver.wait(async () => {
    const t = await driver.findElement(By.tagName("body")).getText();
    return /Ingestion complete/i.test(t);
  }, 10000);
  await screenshot(driver, "07-diagnostics");

  console.log("\n=== Selenium E2E PASSED — real WKWebView, real Rust runtime, real SEC ===");
} catch (e) {
  exitCode = 1;
  console.error("[fail]", e);
  if (driver) {
    try { await screenshot(driver, "fail-state"); } catch { /* ignore */ }
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
