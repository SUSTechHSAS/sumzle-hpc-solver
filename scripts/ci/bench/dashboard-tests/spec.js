/* Playwright-driven verification for the benchmark history dashboard.
 *
 * Covers:
 *   - issue #44: changing one filter must not reset other filters to their
 *     defaults when the previous selection is still valid
 *   - issue #46: multiple selected views overlay in one chart with a legend
 *   - URL round-trip persistence, mixed-unit normalization, table View column
 *
 * Run from the repository root:
 *   node scripts/ci/bench/dashboard-tests/spec.js
 */
"use strict";

const http = require("node:http");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");

/* Playwright is a frontend dev dependency; fall back to its install location
   when run from the repository root without a root-level install. */
let chromium;
try {
  ({ chromium } = require("playwright"));
} catch {
  ({ chromium } = require(path.join(__dirname, "..", "..", "..", "..", "frontend", "node_modules", "playwright")));
}

/* ── Minimal assertion helper ── */

const results = [];
function check(name, condition, detail = "") {
  results.push({ name, ok: Boolean(condition), detail });
  console.log(`${condition ? "PASS" : "FAIL"}  ${name}${detail ? `  (${detail})` : ""}`);
}

/* ── Fixture: 13 runs across main / PR #45 / PR #48 ── */

const DAY = 24 * 3600 * 1000;
const iso = (ms) => new Date(ms).toISOString().replace(/\.\d+Z$/, "Z");

function metric(suite, name, value, unit, params, lowerIsBetter = true) {
  return { suite, metric: name, value, unit, params, lower_is_better: lowerIsBetter };
}

function run(branch, sha, generatedAt, metrics, prNumber) {
  const out = {
    branch,
    sha,
    generated_at: generatedAt,
    run: { id: String(Math.random()).slice(2), url: `https://github.com/SUSTechHSAS/sumzle-hpc-solver/actions/runs/${Math.floor(Math.random() * 1e6)}` },
    metrics,
  };
  if (prNumber) out.pull_request = { number: prNumber, title: `PR ${prNumber}` };
  return out;
}

function makeFixture() {
  const t0 = Date.parse("2026-06-20T00:00:00Z");
  const runs = [];
  for (let i = 0; i < 6; i += 1) {
    runs.push(run("main", `aaaaaa${i}`, iso(t0 + i * DAY), [
      metric("cli", "time", 4200 - 80 * i, "ns", { length: 7 }),
      metric("cli", "solutions", 54 + i, "count", { length: 7 }, false),
      metric("topn", "time", 96 - 3 * i, "ms", { length: 7, n: 10 }),
      metric("topn", "speed", 2000 + 40 * i, "expr/s", { length: 7, n: 10 }, false),
    ]));
  }
  for (let i = 0; i < 4; i += 1) {
    runs.push(run("feature-x", `bbbbbb${i}`, iso(t0 + 1.5 * DAY + i * DAY), [
      metric("cli", "time", 4400 - 100 * i, "ns", { length: 7 }),
      metric("cli", "solutions", 55 + i, "count", { length: 7 }, false),
      metric("topn", "time", 100 - 4 * i, "ms", { length: 7, n: 10 }),
      metric("topn", "speed", 2100 + 30 * i, "expr/s", { length: 7, n: 10 }, false),
    ], 45));
  }
  for (let i = 0; i < 3; i += 1) {
    runs.push(run("feature-y", `cccccc${i}`, iso(t0 + 2.25 * DAY + i * DAY), [
      metric("cli", "time", 5000 - 60 * i, "ns", { length: 8 }),
      metric("cli", "solutions", 60 + i, "count", { length: 8 }, false),
    ], 48));
  }
  return { schema_version: 1, runs };
}

/* ── Static site + server ── */

function buildSite() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "bench-dash-"));
  const dashboardDir = path.join(__dirname, "..", "dashboard");
  for (const file of ["index.html", "app.js", "styles.css"]) {
    fs.copyFileSync(path.join(dashboardDir, file), path.join(dir, file));
  }
  fs.mkdirSync(path.join(dir, "data"));
  fs.writeFileSync(path.join(dir, "data", "history.json"), JSON.stringify(makeFixture(), null, 2));
  return dir;
}

function serve(dir) {
  const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json" };
  const server = http.createServer((req, res) => {
    const urlPath = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    const filePath = path.normalize(path.join(dir, urlPath === "/" ? "index.html" : urlPath));
    if (!filePath.startsWith(dir)) {
      res.writeHead(403).end();
      return;
    }
    fs.readFile(filePath, (err, data) => {
      if (err) {
        res.writeHead(404).end();
        return;
      }
      res.writeHead(200, { "content-type": types[path.extname(filePath)] || "application/octet-stream" });
      res.end(data);
    });
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port }));
  });
}

/* ── Page helpers ── */

async function selectValues(page, values) {
  await page.evaluate((vals) => {
    const ids = ["series-filter", "suite-filter", "metric-filter", "case-filter"];
    const names = ["series", "suite", "metric", "case"];
    ids.forEach((id, i) => {
      const wanted = vals[names[i]];
      if (wanted === undefined) return;
      const select = document.getElementById(id);
      for (const opt of select.options) opt.selected = wanted.includes(opt.value);
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
  }, values);
}

async function selectedValues(page, id) {
  return page.evaluate((selectId) => Array.from(document.getElementById(selectId).selectedOptions).map((o) => o.value), id);
}

async function main() {
  const dir = buildSite();
  const { server, port } = await serve(dir);
  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (err) => errors.push(`pageerror: ${err.message}`));
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(`console: ${msg.text()}`);
  });

  try {
    await page.goto(`http://127.0.0.1:${port}/`);
    await page.waitForSelector("#series-filter option");

    /* ── Initial load ── */
    check("dashboard loads with series options", (await page.locator("#series-filter option").count()) === 3);
    check("legend rendered", await page.locator("#legend").isVisible());
    check(
      "single default view",
      (await page.locator("#legend .legend-item").count()) === 1,
      "defaults: main + cli + first metric/case",
    );

    /* ── Click-to-toggle on multi-select (mousedown on an option) ── */
    await page.dispatchEvent('select#series-filter option[value="PR #45"]', "mousedown");
    let seriesSel = await selectedValues(page, "series-filter");
    check("click-to-toggle adds a series", seriesSel.includes("main") && seriesSel.includes("PR #45"), seriesSel.join("|"));

    /* ── Issue #44: preserve metric + case when another filter changes ── */
    await selectValues(page, { series: ["main"], suite: ["cli"], metric: ["time"], case: ["length=7"] });
    await selectValues(page, { series: ["main", "PR #45"] });
    let metricSel = await selectedValues(page, "metric-filter");
    let caseSel = await selectedValues(page, "case-filter");
    check(
      "#44 metric preserved when series changes",
      metricSel.length === 1 && metricSel[0] === "time",
      metricSel.join("|"),
    );
    check(
      "#44 case preserved when series changes",
      caseSel.length === 1 && caseSel[0] === "length=7",
      caseSel.join("|"),
    );
    check("#44 overlay shows 2 lines", (await page.locator("svg path.chart-line").count()) === 2);

    /* Same for a suite change where the metric still exists */
    await selectValues(page, { series: ["main"], suite: ["cli"], metric: ["time"], case: ["length=7"] });
    await selectValues(page, { suite: ["topn"] });
    metricSel = await selectedValues(page, "metric-filter");
    caseSel = await selectedValues(page, "case-filter");
    check(
      "#44 metric preserved when suite changes",
      metricSel.length === 1 && metricSel[0] === "time",
      metricSel.join("|"),
    );
    check(
      "#44 case falls back only when invalid",
      caseSel.length === 1 && caseSel[0] === "length=7, n=10",
      caseSel.join("|"),
    );

    /* Invalid selection pruned to a valid default */
    await selectValues(page, { series: ["PR #48"], suite: ["cli"], metric: ["time"], case: ["length=7"] });
    metricSel = await selectedValues(page, "metric-filter");
    caseSel = await selectedValues(page, "case-filter");
    check("#44 metric still valid for PR #48", metricSel.length === 1 && metricSel[0] === "time", metricSel.join("|"));
    check("#44 invalid case pruned to length=8", caseSel.length === 1 && caseSel[0] === "length=8", caseSel.join("|"));

    /* ── Issue #46: overlay views ── */
    await selectValues(page, { series: ["main", "PR #45"], suite: ["cli"], metric: ["time"], case: ["length=7"] });
    check("#46 legend shows 2 views", (await page.locator("#legend .legend-item").count()) === 2);
    check("#46 chart draws 2 overlay lines", (await page.locator("svg path.chart-line").count()) === 2);
    const lineColors = await page.$$eval("svg path.chart-line", (els) => els.map((e) => e.style.getPropertyValue("--view-color")));
    check("#46 views use distinct colors", new Set(lineColors).size === 2, lineColors.join(", "));

    /* Legend toggle hides a view */
    const tableRowsBefore = await page.locator("#run-table tr").count();
    await page.locator("#legend .legend-item").nth(0).click();
    check("#46 legend toggle hides line", (await page.locator("svg path.chart-line").count()) === 1);
    check("#46 legend aria-pressed reflects state", (await page.locator("#legend .legend-item").nth(0).getAttribute("aria-pressed")) === "false");
    check("#46 table rows shrink with hidden view", (await page.locator("#run-table tr").count()) < tableRowsBefore);
    await page.locator("#legend .legend-item").nth(0).click();
    check("#46 legend toggle restores line", (await page.locator("svg path.chart-line").count()) === 2);

    /* Mixed units → normalized axis */
    await selectValues(page, { series: ["main"], suite: ["cli"], metric: ["time", "solutions"], case: ["length=7"] });
    const subtitle = await page.textContent("#chart-subtitle");
    check("#46 mixed units normalize", subtitle.includes("normalized"), subtitle);
    const axisLabels = await page.$$eval("text.chart-axis-label", (els) => els.map((e) => e.textContent));
    check("#46 normalized axis labels in %", axisLabels.includes("100%") && axisLabels.includes("0%"), axisLabels.join(" | "));

    /* Table View column */
    await selectValues(page, { series: ["main", "PR #45"], suite: ["cli"], metric: ["time"], case: ["length=7"] });
    const headerCount = await page.locator("#run-table").count() === 0
      ? 0
      : await page.locator("thead th").count();
    check("table has 6 columns incl. View", headerCount === 6, String(headerCount));
    check("table has rows for both views", (await page.locator("#run-table tr").count()) >= 2);

    /* URL persistence + reload */
    const urlAfterSelect = page.url();
    check(
      "selection serialized to URL",
      /series=PR\+%2345%7Cmain/.test(urlAfterSelect) && /metric=time/.test(urlAfterSelect) && /case=length%3D7/.test(urlAfterSelect),
      urlAfterSelect,
    );
    await page.reload();
    await page.waitForSelector("#legend .legend-item");
    seriesSel = await selectedValues(page, "series-filter");
    metricSel = await selectedValues(page, "metric-filter");
    caseSel = await selectedValues(page, "case-filter");
    check(
      "selection restored after reload",
      seriesSel.includes("main") && seriesSel.includes("PR #45") && metricSel.length === 1 && metricSel[0] === "time" && caseSel[0] === "length=7",
      seriesSel.join("|"),
    );
    check("overlay restored after reload", (await page.locator("svg path.chart-line").count()) === 2);

    /* Reset button restores defaults */
    await page.click("#reset-filters");
    seriesSel = await selectedValues(page, "series-filter");
    check("reset restores default series", seriesSel.length === 1 && seriesSel[0] === "main", seriesSel.join("|"));
    check("reset restores single view", (await page.locator("#legend .legend-item").count()) === 1);

    /* Screenshots for visual review */
    await selectValues(page, { series: ["main", "PR #45"], suite: ["cli"], metric: ["time"], case: ["length=7"] });
    await page.screenshot({ path: "/tmp/bench-dash-light.png", fullPage: true });
    await page.emulateMedia({ colorScheme: "dark" });
    await page.screenshot({ path: "/tmp/bench-dash-dark.png", fullPage: true });

    check("no console/page errors", errors.length === 0, errors.join("; "));
  } finally {
    await browser.close();
    server.close();
    fs.rmSync(dir, { recursive: true, force: true });
  }

  const failed = results.filter((r) => !r.ok);
  console.log(`\n${results.length - failed.length}/${results.length} checks passed`);
  if (failed.length) {
    console.log("Failed:");
    for (const r of failed) console.log(`  - ${r.name}`);
    process.exitCode = 1;
  }
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
