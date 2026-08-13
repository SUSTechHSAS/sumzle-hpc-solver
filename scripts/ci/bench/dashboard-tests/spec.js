#!/usr/bin/env node
/* Benchmark dashboard end-to-end tests (Playwright).
 *
 * Serves the dashboard from `scripts/ci/bench/dashboard` with a deterministic
 * in-memory fixture (or an on-disk `data/history.json` when one exists, e.g.
 * for a quick sanity check against real data) and verifies:
 *
 *   - filter selections are preserved when other filters change (#44)
 *   - multiple views overlay in one chart with distinct colors (#46)
 *   - legend show/hide, mixed-unit normalization, URL round-trip,
 *     reset button, keyboard probe, empty state, mobile layout
 *   - no console or page errors
 *
 * Usage:
 *   node scripts/ci/bench/dashboard-tests/spec.js
 */

const http = require("node:http");
const fs = require("node:fs");
const path = require("node:path");

let playwright;
try {
  playwright = require("playwright");
} catch {
  playwright = require(path.join(__dirname, "..", "..", "..", "..", "frontend", "node_modules", "playwright"));
}
const { chromium } = playwright;

const DASH_DIR = path.join(__dirname, "..", "dashboard");
const DATA_FILE = path.join(DASH_DIR, "data", "history.json");
const PORT = 8900;

/* ── Deterministic fixture: 2 series × 2 suites × 2 metrics × 2 cases ── */

function fixture() {
  const runs = [];
  const series = [
    { branch: "main" },
    { branch: "feature/bench", pull_request: { number: 10 } },
  ];
  const metrics = [
    { suite: "cli", metric: "time", unit: "ns", params: { length: 3 }, value: 5_000_000 },
    { suite: "cli", metric: "peak_rss", unit: "KiB", params: { length: 3 }, value: 40_000 },
    { suite: "memory", metric: "time", unit: "ns", params: { group: "solve" }, value: 9_000_000 },
    { suite: "memory", metric: "peak_rss", unit: "KiB", params: { group: "solve" }, value: 90_000 },
  ];
  let runId = 100;
  const baseTime = Date.parse("2026-06-01T00:00:00Z");
  series.forEach((s, seriesIndex) => {
    for (let day = 0; day < 6; day += 1) {
      runId += 1;
      runs.push({
        schema_version: 1,
        generated_at: new Date(baseTime + (seriesIndex * 7 + day) * 86400000).toISOString(),
        sha: `abcdef${day}${seriesIndex}1234567890`,
        branch: s.branch,
        pull_request: s.pull_request || null,
        environment: "ubuntu-latest",
        event: "push",
        repository: "SUSTechHSAS/sumzle-hpc-solver",
        run: { id: String(runId), url: `https://github.com/SUSTechHSAS/sumzle-hpc-solver/actions/runs/${runId}`, workflow: "Benchmark" },
        metrics: metrics.map((m) => ({
          suite: m.suite,
          metric: m.metric,
          unit: m.unit,
          params: { ...m.params },
          value: m.value + seriesIndex * 1000 + day * 100,
          lower_is_better: m.metric === "time",
        })),
      });
    }
  });
  return { schema_version: 1, runs };
}

const fixtureJson = JSON.stringify(fixture());

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
};

function startServer() {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const urlPath = decodeURIComponent(new URL(req.url, "http://x").pathname);
      if (urlPath === "/data/history.json") {
        const onDisk = fs.existsSync(DATA_FILE) ? fs.readFileSync(DATA_FILE, "utf8") : null;
        const body = onDisk || fixtureJson;
        res.writeHead(200, { "Content-Type": MIME[".json"], "Cache-Control": "no-store" });
        res.end(body);
        return;
      }
      const safePath = path.normalize(urlPath).replace(/^(\.\.[/\\])+/, "");
      let file = path.join(DASH_DIR, safePath);
      if (file.startsWith(DASH_DIR) && fs.existsSync(file) && fs.statSync(file).isDirectory()) {
        file = path.join(file, "index.html");
      }
      if (!file.startsWith(DASH_DIR) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
        res.writeHead(404);
        res.end("not found");
        return;
      }
      res.writeHead(200, { "Content-Type": MIME[path.extname(file)] || "application/octet-stream" });
      res.end(fs.readFileSync(file));
    });
    server.listen(PORT, "127.0.0.1", () => resolve(server));
  });
}

/* ── Minimal test harness ── */

let passed = 0;
const failures = [];

async function test(name, fn) {
  try {
    await fn();
    passed += 1;
    console.log(`  ok  ${name}`);
  } catch (error) {
    failures.push({ name, error });
    console.error(`FAIL  ${name}\n      ${String(error).split("\n").join("\n      ")}`);
  }
}

function assert(condition, message) {
  if (!condition) throw new Error(message || "assertion failed");
}

async function main() {
  const server = await startServer();
  const base = `http://127.0.0.1:${PORT}/`;
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  page.on("pageerror", (e) => errors.push(String(e)));

  const chips = (dim) => page.locator(`#${dim}-chips .chip`);
  const pressedValues = async (dim) =>
    (await page.locator(`#${dim}-chips .chip[aria-pressed="true"]`).allTextContents()).sort();
  const clickChip = async (dim, value) => {
    await chips(dim).filter({ hasText: value }).first().click();
  };
  const lines = () => page.locator("#chart .chart-line");
  const waitLoaded = () =>
    page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("runs"));
  const reset = async () => {
    await page.locator("#reset-filters").click();
    await waitLoaded();
  };
  const expectNoErrors = () =>
    assert(errors.length === 0, `console/page errors: ${errors.join(" | ")}`);

  console.log("Benchmark dashboard tests");

  await test("page loads with fixture data and no errors", async () => {
    await page.goto(base, { waitUntil: "networkidle" });
    await waitLoaded();
    assert((await page.textContent("#summary")).includes("12 runs"), "summary should report 12 runs");
    expectNoErrors();
  });

  await test("defaults: main + cli + first metric/case, single line, legend hidden", async () => {
    assert((await pressedValues("series")).join() === "main", `series=${await pressedValues("series")}`);
    assert((await pressedValues("suite")).join() === "cli", `suites=${await pressedValues("suite")}`);
    /* sorted fixture metrics: peak_rss < time */
    assert((await pressedValues("metric")).join() === "peak_rss", `metrics=${await pressedValues("metric")}`);
    assert((await pressedValues("case")).join() === "length=3", `cases=${await pressedValues("case")}`);
    assert((await lines().count()) === 1, `expected 1 line, got ${await lines().count()}`);
    assert(await page.locator("#legend").isHidden(), "legend should be hidden for a single view");
    assert((await page.locator("#run-table tr").count()) === 6, "table should show 6 runs");
    expectNoErrors();
  });

  await test("#44: metric selection survives a suite change while still valid", async () => {
    await reset();
    /* default metric is peak_rss — deselect it, then pick time */
    await clickChip("metric", "peak_rss");
    await clickChip("metric", "time");
    assert((await pressedValues("metric")).join() === "time", "time selected");
    await clickChip("suite", "memory");
    /* time also exists in memory — selection must be preserved, not reset */
    assert((await pressedValues("metric")).join() === "time", "metric must NOT reset to default on suite change (#44)");
    /* length=3 still exists in cli (still selected) — kept as well */
    assert((await pressedValues("case")).join() === "length=3", "case kept while still valid");
    expectNoErrors();
  });

  await test("#44: case selection survives while still valid, falls back only when invalid", async () => {
    await reset();
    await clickChip("metric", "peak_rss");
    await clickChip("metric", "time");
    /* cli + time → case length=3 */
    assert((await pressedValues("case")).join() === "length=3", "case length=3 selected");
    /* add memory: time exists there, case group=solve — length=3 stays valid (cli still selected) */
    await clickChip("suite", "memory");
    assert((await pressedValues("case")).join() === "length=3", "case kept while valid across suites");
    /* drop cli: length=3 no longer exists → falls back to the first valid case */
    await clickChip("suite", "cli");
    assert((await pressedValues("case")).join() === "group=solve", "case falls back to a valid option when pruned");
    expectNoErrors();
  });

  await test("#46: two views overlay as two colored lines with legend", async () => {
    await reset();
    await clickChip("series", "PR #10");
    assert((await pressedValues("series")).sort().join() === "PR #10,main", "both series selected");
    assert((await lines().count()) === 2, `expected 2 lines, got ${await lines().count()}`);
    assert(!(await page.locator("#legend").isHidden()), "legend should be visible with 2+ views");
    assert((await page.locator("#legend .legend-item").count()) === 2, "legend should list 2 views");
    const strokes = await lines().evaluateAll((els) => els.map((el) => el.style.stroke));
    assert(new Set(strokes).size === 2, "the two lines must use distinct colors");
    assert((await page.textContent("#chart-subtitle")).includes("2 views"), "subtitle should report 2 views");
    expectNoErrors();
  });

  await test("legend toggle hides and restores a view", async () => {
    await reset();
    await clickChip("series", "PR #10");
    await page.locator("#legend .legend-item button").first().click();
    assert((await lines().count()) === 1, "hiding a view should leave 1 line");
    assert((await page.textContent("#chart-subtitle")).includes("1 hidden"), "subtitle should note the hidden view");
    await page.locator("#legend .legend-item button").first().click();
    assert((await lines().count()) === 2, "re-showing the view should restore 2 lines");
    expectNoErrors();
  });

  await test("mixed units render on a normalized axis", async () => {
    await reset();
    /* cli has time (ns) and peak_rss (KiB) → mixed units */
    await clickChip("metric", "time");
    assert((await pressedValues("metric")).sort().join() === "peak_rss,time", "both metrics selected");
    assert((await lines().count()) === 2, "expected 2 views");
    assert((await page.textContent("#chart-subtitle")).includes("normalized"), "subtitle should mention normalization");
    expectNoErrors();
  });

  await test("keyboard probe compares all views at a time", async () => {
    await reset();
    await clickChip("series", "PR #10");
    assert((await lines().count()) === 2, "precondition: 2 views");
    await page.locator("#chart .chart-probe").focus();
    assert((await page.locator(".tooltip-row").count()) === 2, "tooltip should list every view");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    assert((await page.locator(".tooltip-row").count()) === 2, "tooltip follows the probe");
    await page.keyboard.press("Escape");
    expectNoErrors();
  });

  await test("selection persists in URL and survives reload", async () => {
    await reset();
    await clickChip("series", "PR #10");
    await clickChip("metric", "time");
    const url = page.url();
    assert(url.includes("series=") && url.includes("metric="), `URL should carry selection: ${url}`);
    await page.reload({ waitUntil: "networkidle" });
    await waitLoaded();
    assert((await pressedValues("series")).sort().join() === "PR #10,main", "series restored after reload");
    assert((await pressedValues("metric")).sort().join() === "peak_rss,time", "metric restored after reload");
    expectNoErrors();
  });

  await test("reset button clears URL and restores defaults", async () => {
    await page.locator("#reset-filters").click();
    await waitLoaded();
    assert(!page.url().includes("series="), "URL should be cleared after reset");
    assert((await pressedValues("series")).join() === "main", `series=${await pressedValues("series")}`);
    assert((await pressedValues("suite")).join() === "cli", `suite=${await pressedValues("suite")}`);
    assert((await pressedValues("metric")).join() === "peak_rss", `metric=${await pressedValues("metric")}`);
    assert((await lines().count()) === 1, "single line after reset");
    expectNoErrors();
  });

  await test("legacy ?pr=10 param selects the PR series", async () => {
    await page.goto(`${base}?pr=10`, { waitUntil: "networkidle" });
    await waitLoaded();
    assert((await pressedValues("series")).join() === "PR #10", `series=${await pressedValues("series")}`);
    await page.goto(base, { waitUntil: "networkidle" });
    await waitLoaded();
    expectNoErrors();
  });

  await test("deselecting everything shows the empty state", async () => {
    await reset();
    for (const value of await pressedValues("series")) {
      await clickChip("series", value);
    }
    assert((await lines().count()) === 0, "no lines when nothing is selected");
    assert(await page.locator("#chart .chart-empty-text").isVisible(), "chart empty state visible");
    assert(await page.locator("#table-empty").isVisible(), "table empty state visible");
    await reset();
    expectNoErrors();
  });

  await test("mobile viewport: chips wrap, no horizontal overflow", async () => {
    await page.setViewportSize({ width: 390, height: 844 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    assert(overflow <= 1, `horizontal overflow of ${overflow}px on mobile`);
    assert((await chips("series").count()) >= 2, "series chips present on mobile");
    await page.setViewportSize({ width: 1280, height: 900 });
    expectNoErrors();
  });

  console.log(`\n${passed} passed, ${failures.length} failed`);
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
  if (failures.length) process.exitCode = 1;
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
