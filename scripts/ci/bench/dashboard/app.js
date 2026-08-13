/* ── Touch/device detection for contextual copy ── */
if (!window.matchMedia("(hover: hover)").matches) {
  const tipText = document.getElementById("chart-tip-text");
  if (tipText) tipText.textContent = "Tap data points for details — use arrow keys to navigate the chart";
}

const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
const prefersReducedMotionNow = () => prefersReducedMotion.matches;
// Listen for runtime changes to OS accessibility setting
prefersReducedMotion.addEventListener("change", () => {
  // CSS handles the visual reset; this listener ensures
  // future JS animation decisions reflect the current state.
});

/* ── Constants ── */

/* Distinct, mid-lightness colors that hold up in light and dark mode. */
const PALETTE = [
  "oklch(0.6 0.16 25)",   /* red-orange   */
  "oklch(0.62 0.15 90)",  /* gold         */
  "oklch(0.6 0.16 150)",  /* green        */
  "oklch(0.6 0.15 200)",  /* teal         */
  "oklch(0.58 0.17 260)", /* blue         */
  "oklch(0.58 0.16 310)", /* purple       */
  "oklch(0.64 0.13 350)", /* pink         */
  "oklch(0.64 0.14 45)",  /* amber        */
  "oklch(0.62 0.14 120)", /* lime         */
  "oklch(0.6 0.13 230)",  /* azure        */
  "oklch(0.62 0.14 290)", /* violet       */
  "oklch(0.58 0.11 15)",  /* brick        */
];

const MAX_VIEWS = 24;
const SELECTION_STORAGE_KEY = "bench-dash-selection-v2";
const RANGE_STORAGE_KEY = "bench-dash-range-v2";

const state = {
  history: { runs: [] },
  points: [],
  /* One set per filter dimension. Toggling one chip never touches the other
     dimensions, so changing a filter can't reset the others (issue #44). */
  selection: {
    series: new Set(),
    suites: new Set(),
    metrics: new Set(),
    cases: new Set(),
  },
  /* Views the user turned off via the legend (session-only). */
  hiddenViews: new Set(),
};

const safeStorage = {
  get(key) {
    try {
      return window.localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  set(key, value) {
    try {
      window.localStorage.setItem(key, value);
    } catch {
      /* Ignore storage failures in private or locked-down browsing modes. */
    }
  },
};

const els = {
  summary: document.getElementById("summary"),
  welcome: document.getElementById("welcome"),
  seriesChips: document.getElementById("series-chips"),
  suiteChips: document.getElementById("suite-chips"),
  metricChips: document.getElementById("metric-chips"),
  caseChips: document.getElementById("case-chips"),
  range: document.getElementById("range-filter"),
  rangeOutput: document.getElementById("range-output"),
  resetFilters: document.getElementById("reset-filters"),
  chart: document.getElementById("chart"),
  chartTitle: document.getElementById("chart-title"),
  chartSubtitle: document.getElementById("chart-subtitle"),
  tooltip: document.getElementById("tooltip"),
  chartStatus: document.getElementById("chart-status"),
  chartTip: document.getElementById("chart-tip"),
  chartTipDismiss: document.getElementById("chart-tip-dismiss"),
  legend: document.getElementById("legend"),
  table: document.getElementById("run-table"),
  tableEmpty: document.getElementById("table-empty"),
  rangeContext: document.getElementById("range-context"),
};

/* ── Helpers ── */

function seriesName(run) {
  if (run.pull_request?.number) {
    return `PR #${run.pull_request.number}`;
  }
  return run.branch || "main";
}

function caseName(metric) {
  const params = metric.params || {};
  const parts = Object.keys(params)
    .sort()
    .map((key) => `${key}=${params[key]}`);
  const raw = parts.length ? parts.join(", ") : "default";
  /* Truncate very long case names to prevent layout overflow */
  return raw.length > 80 ? raw.slice(0, 77) + "…" : raw;
}

function pointTime(run) {
  const t = Date.parse(run.generated_at || "");
  return Number.isFinite(t) ? t : 0;
}

function buildPoints() {
  state.points = [];
  for (const run of state.history?.runs || []) {
    if (!run) continue;
    for (const metric of run.metrics || []) {
      if (!metric) continue;
      const value = Number(metric.value);
      if (!Number.isFinite(value)) continue;
      state.points.push({
        run,
        metric,
        series: seriesName(run),
        suite: metric.suite,
        metricName: metric.metric,
        caseName: caseName(metric),
        value,
        unit: metric.unit,
        generatedAt: run.generated_at || "",
        time: pointTime(run),
      });
    }
  }
}

function sortedUnique(values) {
  return [...new Set(values)].sort();
}

/* ── Available option sets ──
   Series and suite options are global. Metric options depend on the selected
   series + suites, and case options on the selected series + suites + metrics.
   Selections that stay valid are kept; only values that no longer exist are
   pruned (issue #44: changing one filter never resets the others). */

function allSeries() {
  return sortedUnique(state.points.map((point) => point.series));
}

function allSuites() {
  return sortedUnique(state.points.map((point) => point.suite));
}

function validMetrics() {
  const sel = state.selection;
  return sortedUnique(
    state.points
      .filter((point) => sel.series.has(point.series) && sel.suites.has(point.suite))
      .map((point) => point.metricName),
  );
}

function validCases() {
  const sel = state.selection;
  return sortedUnique(
    state.points
      .filter(
        (point) =>
          sel.series.has(point.series) &&
          sel.suites.has(point.suite) &&
          sel.metrics.has(point.metricName),
      )
      .map((point) => point.caseName),
  );
}

function prune(selection, valid, wasEmpty) {
  const kept = new Set([...selection].filter((value) => valid.includes(value)));
  /* Only fall back to the first available option when a previous non-empty
     selection became fully invalid (e.g. a scope change removed it). A
     deliberately emptied dimension stays empty. */
  if (!kept.size && !wasEmpty && valid.length) kept.add(valid[0]);
  return kept;
}

function pruneSelection() {
  const sel = state.selection;
  const metricsWereEmpty = sel.metrics.size === 0;
  const casesWereEmpty = sel.cases.size === 0;
  sel.metrics = prune(sel.metrics, validMetrics(), metricsWereEmpty);
  sel.cases = prune(sel.cases, validCases(), casesWereEmpty);
}

/* ── Chip controls ── */

const CHIP_GROUPS = [
  { dim: "series", container: () => els.seriesChips, values: allSeries },
  { dim: "suites", container: () => els.suiteChips, values: allSuites },
  { dim: "metrics", container: () => els.metricChips, values: validMetrics },
  { dim: "cases", container: () => els.caseChips, values: validCases },
];

function renderChips() {
  for (const group of CHIP_GROUPS) {
    const container = group.container();
    const values = group.values();
    container.replaceChildren();
    if (!values.length) {
      /* Placeholder when no data — teaches the interface even when empty */
      const placeholder = document.createElement("button");
      placeholder.type = "button";
      placeholder.className = "chip";
      placeholder.disabled = true;
      placeholder.textContent = "No data";
      container.appendChild(placeholder);
      continue;
    }
    const fragment = document.createDocumentFragment();
    for (const value of values) {
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "chip";
      chip.dataset.dim = group.dim;
      chip.dataset.value = value;
      chip.setAttribute("aria-pressed", String(state.selection[group.dim].has(value)));
      chip.textContent = value;
      chip.title = value;
      fragment.appendChild(chip);
    }
    container.appendChild(fragment);
  }
}

function toggleSelection(dim, value) {
  const set = state.selection[dim];
  const adding = !set.has(value);
  if (adding) {
    set.add(value);
  } else {
    set.delete(value);
  }
  /* Lower dimensions depend on this one — keep whatever is still valid.
     A deliberately emptied dimension is never auto-refilled. */
  pruneSelection();
  /* When adding to an upper dimension, give an empty dependent dimension its
     first valid option so the chart doesn't stay blank. */
  if (adding) seedDependents(dim);
  persistSelection();
  render();
}

function seedDependents(dim) {
  const sel = state.selection;
  if ((dim === "series" || dim === "suites") && !sel.metrics.size) {
    sel.metrics = prune(sel.metrics, validMetrics(), false);
  }
  if ((dim === "series" || dim === "suites" || dim === "metrics") && !sel.cases.size) {
    sel.cases = prune(sel.cases, validCases(), false);
  }
}

document.querySelector(".controls").addEventListener("click", (event) => {
  const chip = event.target.closest(".chip");
  if (!chip || chip.disabled || !chip.dataset.dim) return;
  toggleSelection(chip.dataset.dim, chip.dataset.value);
});

/* ── Selection persistence (URL + localStorage) ──
   "|" is the separator because case names legitimately contain ", ". */

function persistSelection() {
  const sel = state.selection;
  const params = new URLSearchParams();
  const join = (values) => [...values].sort().join("|");
  params.set("series", join(sel.series));
  params.set("suite", join(sel.suites));
  params.set("metric", join(sel.metrics));
  params.set("case", join(sel.cases));
  const qs = params.toString();
  window.history.replaceState(null, "", qs ? `?${qs}` : window.location.pathname);
  safeStorage.set(
    SELECTION_STORAGE_KEY,
    JSON.stringify({
      series: [...sel.series],
      suites: [...sel.suites],
      metrics: [...sel.metrics],
      cases: [...sel.cases],
    }),
  );
}

function parseStoredSelection() {
  const raw = safeStorage.get(SELECTION_STORAGE_KEY);
  if (!raw) return null;
  try {
    const obj = JSON.parse(raw);
    if (!obj || typeof obj !== "object") return null;
    const toArray = (value) =>
      Array.isArray(value) ? value.filter((item) => typeof item === "string") : null;
    const stored = {
      series: toArray(obj.series),
      suites: toArray(obj.suites),
      metrics: toArray(obj.metrics),
      cases: toArray(obj.cases),
    };
    return stored.series || stored.suites || stored.metrics || stored.cases ? stored : null;
  } catch {
    return null;
  }
}

function paramValues(params, name) {
  const raw = params.get(name);
  if (!raw) return null;
  const values = raw
    .split("|")
    .map((value) => value.trim())
    .filter(Boolean);
  return values.length ? values : null;
}

function initSelection() {
  const params = new URLSearchParams(window.location.search);
  const stored = parseStoredSelection();

  /* Precedence: explicit URL params → legacy ?pr=&suite=&metric= params →
     localStorage → defaults. */
  const seriesRequested =
    paramValues(params, "series") ||
    (params.get("pr") ? [`PR #${params.get("pr")}`] : null) ||
    (stored && stored.series);
  const suitesRequested = paramValues(params, "suite") || (stored && stored.suites);
  const metricsRequested = paramValues(params, "metric") || (stored && stored.metrics);
  const casesRequested = paramValues(params, "case") || (stored && stored.cases);

  const sel = state.selection;

  const seriesAll = allSeries();
  sel.series = new Set((seriesRequested || []).filter((value) => seriesAll.includes(value)));
  if (!sel.series.size && seriesAll.length) {
    sel.series.add(seriesAll.includes("main") ? "main" : seriesAll[0]);
  }

  const suitesAll = allSuites();
  sel.suites = new Set((suitesRequested || []).filter((value) => suitesAll.includes(value)));
  if (!sel.suites.size && suitesAll.length) {
    sel.suites.add(suitesAll.includes("cli") ? "cli" : suitesAll[0]);
  }

  const metricValid = validMetrics();
  sel.metrics = new Set((metricsRequested || []).filter((value) => metricValid.includes(value)));
  if (!sel.metrics.size && metricValid.length) sel.metrics.add(metricValid[0]);

  const caseValid = validCases();
  sel.cases = new Set((casesRequested || []).filter((value) => caseValid.includes(value)));
  if (!sel.cases.size && caseValid.length) sel.cases.add(caseValid[0]);

  const storedRange = Number(safeStorage.get(RANGE_STORAGE_KEY));
  if (Number.isFinite(storedRange) && storedRange >= Number(els.range.min) && storedRange <= Number(els.range.max)) {
    els.range.value = String(storedRange);
  }
}

/* ── Views ──
   A view is one (series, suite, metric, case) combination. All selected
   combinations are overlaid in the same chart (issue #46). */

function viewKey(series, suite, metric, caseValue) {
  return `${series}||${suite}||${metric}||${caseValue}`;
}

function shortViewLabel(view) {
  return `${view.series} · ${view.suite} / ${view.metric} · ${view.caseName}`;
}

function buildViews() {
  const sel = state.selection;
  const recent = Math.max(1, Number(els.range.value) || 40);
  const views = [];
  let capped = false;
  for (const series of [...sel.series].sort()) {
    for (const suite of [...sel.suites].sort()) {
      for (const metric of [...sel.metrics].sort()) {
        for (const caseValue of [...sel.cases].sort()) {
          if (views.length >= MAX_VIEWS) {
            capped = true;
            return { views, capped };
          }
          const points = state.points
            .filter(
              (point) =>
                point.series === series &&
                point.suite === suite &&
                point.metricName === metric &&
                point.caseName === caseValue,
            )
            .sort((a, b) => a.time - b.time)
            .slice(-recent);
          if (!points.length) continue;
          const key = viewKey(series, suite, metric, caseValue);
          views.push({
            key,
            series,
            suite,
            metric,
            caseName: caseValue,
            unit: points[0].unit,
            points,
            color: PALETTE[views.length % PALETTE.length],
          });
        }
      }
    }
  }
  return { views, capped };
}

function visibleViews() {
  const { views, capped } = buildViews();
  return { allViews: views, views: views.filter((view) => !state.hiddenViews.has(view.key)), capped };
}

/* ── Chart data context ── */

function viewScale(view) {
  const values = view.points.map((point) => point.value);
  let min = safeMin(values);
  let max = safeMax(values);
  if (min === max) {
    if (min === 0) {
      max = 1;
    } else {
      const delta = Math.abs(min) * 0.05;
      min -= delta;
      max += delta;
    }
  }
  return { min, max };
}

function computeContext(views) {
  const units = new Set(views.map((view) => view.unit));
  const unitUniform = units.size <= 1;
  const allPoints = views.flatMap((view) => view.points);
  const ctx = {
    views,
    unitUniform,
    unit: unitUniform && views.length ? views[0].unit : "",
    tMin: 0,
    tMax: 0,
    absMin: 0,
    absMax: 0,
    scales: views.map((view) => viewScale(view)),
  };
  if (allPoints.length) {
    ctx.tMin = safeMin(allPoints.map((point) => point.time));
    ctx.tMax = safeMax(allPoints.map((point) => point.time));
    if (unitUniform) {
      const values = allPoints.map((point) => point.value);
      let min = safeMin(values);
      let max = safeMax(values);
      if (min === max) {
        if (min === 0) {
          max = 1;
        } else {
          const delta = Math.abs(min) * 0.05;
          min -= delta;
          max += delta;
        }
      }
      const pad = (max - min) * 0.08;
      ctx.absMin = min - pad;
      ctx.absMax = max + pad;
    }
  }
  return ctx;
}

/* ── Formatting ── */

function formatValue(value, unit) {
  if (unit === "ns") {
    if (value >= 1e9) return `${(value / 1e9).toFixed(2)} s`;
    if (value >= 1e6) return `${(value / 1e6).toFixed(1)} ms`;
    if (value >= 1e3) return `${(value / 1e3).toFixed(1)} \u00b5s`;
  }
  return `${Number.isInteger(value) ? value : value.toFixed(2)}${unit ? ` ${unit}` : ""}`;
}

function formatTick(t, spanMs) {
  const d = new Date(t);
  const pad = (value) => String(value).padStart(2, "0");
  const date = `${d.getMonth() + 1}/${d.getDate()}`;
  if (spanMs > 2 * 24 * 3600 * 1000) return date;
  return `${date} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function clearSvg(svg) {
  svg.replaceChildren();
}

function safeMin(values) {
  if (!values.length) return 0;
  let result = values[0];
  for (let i = 1; i < values.length; i += 1) {
    if (values[i] < result) result = values[i];
  }
  return result;
}

function safeMax(values) {
  if (!values.length) return 0;
  let result = values[0];
  for (let i = 1; i < values.length; i += 1) {
    if (values[i] > result) result = values[i];
  }
  return result;
}

function isSafeUrl(url) {
  try {
    const parsed = new URL(url, window.location.origin);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

function svgEl(name, attrs = {}) {
  const node = document.createElementNS("http://www.w3.org/2000/svg", name);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  return node;
}

/* ── Smooth curve interpolation (Cardinal spline → cubic Bézier) ── */

function smoothPath(pts, s = 0.4) {
  if (pts.length < 2) return "";
  if (pts.length === 2) return `M ${pts[0][0]} ${pts[0][1]} L ${pts[1][0]} ${pts[1][1]}`;

  let d = `M ${pts[0][0]} ${pts[0][1]}`;

  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[Math.min(pts.length - 1, i + 2)];

    const cp1x = p1[0] + (s * (p2[0] - p0[0])) / 3;
    const cp1y = p1[1] + (s * (p2[1] - p0[1])) / 3;
    const cp2x = p2[0] - (s * (p3[0] - p1[0])) / 3;
    const cp2y = p2[1] - (s * (p3[1] - p1[1])) / 3;

    d += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2[0]} ${p2[1]}`;
  }

  return d;
}

/* ── Chart rendering ── */

let crosshairLine = null;
let probeLine = null;
let probeTimes = [];
let probeIndex = 0;
let currentCtx = null;

function renderChart(views) {
  const svg = els.chart;
  clearSvg(svg);
  crosshairLine = null;
  probeLine = null;
  probeTimes = [];
  probeIndex = 0;
  currentCtx = null;

  const width = svg.clientWidth || 900;
  const height = svg.clientHeight || 420;
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);

  const margin = { top: 24, right: 24, bottom: 64, left: 78 };
  const plotW = Math.max(1, width - margin.left - margin.right);
  const plotH = Math.max(1, height - margin.top - margin.bottom);

  const fragment = document.createDocumentFragment();

  crosshairLine = svgEl("line", {
    x1: 0,
    x2: 0,
    y1: margin.top,
    y2: height - margin.bottom,
    class: "chart-crosshair",
  });
  fragment.appendChild(crosshairLine);

  const ctx = computeContext(views);
  currentCtx = ctx;

  if (!views.length) {
    const emptyRect = svgEl("rect", {
      x: margin.left, y: margin.top, width: plotW, height: plotH,
    });
    emptyRect.classList.add("chart-empty-rect");
    fragment.appendChild(emptyRect);

    const emptyText = svgEl("text", { x: width / 2, y: height / 2 - 4, "text-anchor": "middle" });
    emptyText.classList.add("chart-empty-text");
    emptyText.textContent = "No data for the selected filters";
    fragment.appendChild(emptyText);

    const emptyHint = svgEl("text", { x: width / 2, y: height / 2 + 16, "text-anchor": "middle" });
    emptyHint.classList.add("chart-empty-hint");
    emptyHint.textContent = "Try toggling different branches, suites, metrics, or parameter sets";
    fragment.appendChild(emptyHint);

    svg.appendChild(fragment);
    return;
  }

  const span = ctx.tMax - ctx.tMin;
  const x = (t) => margin.left + (span > 0 ? ((t - ctx.tMin) / span) * plotW : plotW / 2);
  /* Single-unit views share one absolute y-scale; mixed units render on a
     normalized 0–100% axis where each view is scaled to its own range. */
  const y = (value, scale) => {
    const s = ctx.unitUniform ? { min: ctx.absMin, max: ctx.absMax } : scale;
    return margin.top + plotH - ((value - s.min) / Math.max(1e-9, s.max - s.min)) * plotH;
  };

  /* Grid lines + Y-axis labels */
  for (let i = 0; i <= 4; i += 1) {
    const gy = margin.top + (plotH / 4) * i;
    const gridLine = svgEl("line", { x1: margin.left, x2: width - margin.right, y1: gy, y2: gy });
    gridLine.classList.add("chart-grid-line");
    fragment.appendChild(gridLine);
    const label = svgEl("text", { x: margin.left - 12, y: gy + 4, "text-anchor": "end", "font-size": "12" });
    label.classList.add("chart-axis-label");
    label.textContent = ctx.unitUniform
      ? formatValue(ctx.absMax - ((ctx.absMax - ctx.absMin) / 4) * i, ctx.unit)
      : `${100 - i * 25}%`;
    fragment.appendChild(label);
  }

  /* X-axis time labels — shared time axis aligns overlapping views */
  const tickCount = 6;
  for (let i = 0; i < tickCount; i += 1) {
    const t = span > 0 ? ctx.tMin + (span * i) / (tickCount - 1) : ctx.tMin;
    const label = svgEl("text", {
      x: x(t),
      y: height - 24,
      "text-anchor": "middle",
      "font-size": "12",
      "overflow": "hidden",
    });
    label.classList.add("chart-axis-label");
    label.textContent = formatTick(t, span);
    fragment.appendChild(label);
  }

  /* One curve per view. With a single view the dots stay keyboard-navigable
     (arrow keys move between points); with several views, keyboard navigation
     switches to the focusable time probe below. */
  const singleView = views.length === 1;
  ctx.views.forEach((view, viewIndex) => {
    const coords = view.points.map((point) => [x(point.time), y(point.value, ctx.scales[viewIndex])]);

    /* Area fill under the curve */
    const areaPathD = coords.length === 1
      ? `M ${coords[0][0]} ${coords[0][1]} L ${coords[0][0]} ${margin.top + plotH} Z`
      : smoothPath(coords) +
        ` L ${coords[coords.length - 1][0]} ${margin.top + plotH} L ${coords[0][0]} ${margin.top + plotH} Z`;
    const area = svgEl("path", { d: areaPathD, "stroke-width": "0" });
    area.classList.add("chart-area");
    area.style.fill = view.color;
    fragment.appendChild(area);

    /* Line path with draw animation */
    const linePathD = smoothPath(coords);
    if (linePathD) {
      const line = svgEl("path", { d: linePathD, fill: "none", "stroke-width": "2.5" });
      line.classList.add("chart-line");
      line.style.stroke = view.color;
      fragment.appendChild(line);
    }

    /* Data point dots */
    view.points.forEach((point, index) => {
      const cx = x(point.time);
      const cy = y(point.value, ctx.scales[viewIndex]);

      const attrs = { cx, cy, r: "4", fill: view.color };
      if (singleView) {
        attrs.tabindex = "0";
        attrs.role = "graphics-symbol";
        attrs["aria-label"] = `${point.suite} / ${point.metricName}: ${formatValue(point.value, point.unit)}`;
        attrs["aria-describedby"] = "tooltip";
        attrs["data-point"] = index;
      }
      const circle = svgEl("circle", attrs);
      if (!prefersReducedMotionNow()) {
        circle.style.animationDelay = `${Math.min(index * 25, 600)}ms`;
      }
      if (singleView) {
        circle.addEventListener("keydown", (event) => {
          if (event.key === "ArrowRight" || event.key === "ArrowDown") {
            event.preventDefault();
            const next = svg.querySelector(`circle[data-point="${index + 1}"]`);
            if (next) next.focus();
          } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
            event.preventDefault();
            const prev = svg.querySelector(`circle[data-point="${index - 1}"]`);
            if (prev) prev.focus();
          } else if (event.key === "Escape") {
            hideTooltip();
            circle.blur();
          }
        });
      }
      fragment.appendChild(circle);
    });
  });

  /* Keyboard probe for multi-view navigation across the shared time axis */
  if (!singleView) {
    probeTimes = [...new Set(ctx.views.flatMap((view) => view.points.map((point) => point.time)))].sort((a, b) => a - b);
    const probeX = x(probeTimes[0]);
    probeLine = svgEl("line", {
      x1: probeX,
      x2: probeX,
      y1: margin.top,
      y2: height - margin.bottom,
      class: "chart-probe",
      tabindex: "0",
      role: "slider",
      "aria-label": "Time probe — use arrow keys to compare views at a given time",
      "aria-valuemin": "0",
      "aria-valuemax": String(Math.max(0, probeTimes.length - 1)),
      "aria-valuenow": "0",
      "aria-valuetext": formatTick(probeTimes[0], span),
    });
    probeLine.addEventListener("focus", () => {
      showProbeTooltip();
    });
    probeLine.addEventListener("keydown", (event) => {
      if (event.key === "ArrowRight" || event.key === "ArrowDown") {
        event.preventDefault();
        probeIndex = Math.min(probeTimes.length - 1, probeIndex + 1);
        moveProbe();
      } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
        event.preventDefault();
        probeIndex = Math.max(0, probeIndex - 1);
        moveProbe();
      } else if (event.key === "Home") {
        event.preventDefault();
        probeIndex = 0;
        moveProbe();
      } else if (event.key === "End") {
        event.preventDefault();
        probeIndex = probeTimes.length - 1;
        moveProbe();
      } else if (event.key === "Escape") {
        hideTooltip();
        probeLine.blur();
      }
    });
    fragment.appendChild(probeLine);
  }

  svg.appendChild(fragment);

  /* Post-append: line draw animation requires elements in DOM for getTotalLength() */
  if (!prefersReducedMotionNow()) {
    svg.querySelectorAll(".chart-line").forEach((line) => {
      const lineLength = line.getTotalLength();
      line.style.strokeDasharray = lineLength;
      line.style.strokeDashoffset = lineLength;
    });
  }
}

function moveProbe() {
  if (!probeLine || !currentCtx) return;
  const t = probeTimes[probeIndex];
  const margin = { top: 24, right: 24, bottom: 64, left: 78 };
  const width = els.chart.clientWidth || 900;
  const height = els.chart.clientHeight || 420;
  const plotW = Math.max(1, width - margin.left - margin.right);
  const span = currentCtx.tMax - currentCtx.tMin;
  const px = margin.left + (span > 0 ? ((t - currentCtx.tMin) / span) * plotW : plotW / 2);
  probeLine.setAttribute("x1", px);
  probeLine.setAttribute("x2", px);
  probeLine.setAttribute("aria-valuenow", String(probeIndex));
  probeLine.setAttribute("aria-valuetext", formatTick(t, span));
  showProbeTooltip();
}

/* ── Crosshair + grouped tooltip ── */

let crosshairRafPending = false;
let lastCrosshairX = 0;

function nearestPointForView(view, t) {
  let best = null;
  let bestDist = Infinity;
  for (const point of view.points) {
    const dist = Math.abs(point.time - t);
    if (dist < bestDist) {
      bestDist = dist;
      best = point;
    }
  }
  return best;
}

function tooltipEntriesAt(t) {
  if (!currentCtx) return [];
  const entries = [];
  for (const view of currentCtx.views) {
    const point = nearestPointForView(view, t);
    if (!point) continue;
    entries.push({
      color: view.color,
      label: shortViewLabel(view),
      value: formatValue(point.value, point.unit),
      unit: point.unit,
      sha: (point.run.sha || "").slice(0, 7),
      generatedAt: point.generatedAt,
    });
  }
  return entries;
}

function showTooltipAt(xClient, yClient, entries, titleText) {
  const rect = els.chart.parentElement.getBoundingClientRect();
  let tx = xClient - rect.left + 18;
  let ty = yClient - rect.top + 18;

  els.tooltip.textContent = "";

  if (titleText) {
    const titleDiv = document.createElement("div");
    titleDiv.className = "tooltip-title";
    titleDiv.setAttribute("dir", "auto");
    titleDiv.textContent = titleText;
    els.tooltip.appendChild(titleDiv);
  }

  for (const entry of entries) {
    const row = document.createElement("div");
    row.className = "tooltip-row";
    row.title = `${entry.sha} · ${entry.generatedAt}`;

    const swatch = document.createElement("span");
    swatch.className = "tooltip-swatch";
    swatch.style.background = entry.color;
    row.appendChild(swatch);

    const label = document.createElement("span");
    label.className = "tooltip-row-label";
    label.setAttribute("dir", "auto");
    label.textContent = entry.label;
    row.appendChild(label);

    const value = document.createElement("span");
    value.className = "tooltip-row-value";
    value.textContent = entry.value;
    row.appendChild(value);

    els.tooltip.appendChild(row);
  }

  els.tooltip.setAttribute("data-visible", "");
  const tooltipRect = els.tooltip.getBoundingClientRect();
  const maxX = rect.width - tooltipRect.width - 8;
  const maxY = rect.height - tooltipRect.height - 8;
  tx = Math.min(Math.max(0, tx), Math.max(0, maxX));
  ty = Math.min(Math.max(0, ty), Math.max(0, maxY));

  els.tooltip.style.setProperty("--tx", `${tx}px`);
  els.tooltip.style.setProperty("--ty", `${ty}px`);
}

function showPointerTooltip(event) {
  if (!currentCtx || !currentCtx.views.length) return;
  const rect = els.chart.getBoundingClientRect();
  const svgX = ((event.clientX - rect.left) / rect.width) * (els.chart.viewBox.baseVal.width || 900);
  const margin = { left: 78, right: 24 };
  const width = els.chart.viewBox.baseVal.width || 900;
  const plotW = Math.max(1, width - margin.left - margin.right);
  const span = currentCtx.tMax - currentCtx.tMin;
  const t = span > 0 ? currentCtx.tMin + ((svgX - margin.left) / plotW) * span : currentCtx.tMin;
  const entries = tooltipEntriesAt(t);
  if (!entries.length) return;
  showTooltipAt(event.clientX, event.clientY, entries, formatTick(t, span));
}

function showProbeTooltip() {
  if (!probeTimes.length) return;
  const t = probeTimes[probeIndex];
  const entries = tooltipEntriesAt(t);
  if (!entries.length) return;
  const rect = els.chart.getBoundingClientRect();
  const margin = { left: 78, right: 24 };
  const width = els.chart.viewBox.baseVal.width || 900;
  const plotW = Math.max(1, width - margin.left - margin.right);
  const span = currentCtx.tMax - currentCtx.tMin;
  const px = margin.left + (span > 0 ? ((t - currentCtx.tMin) / span) * plotW : plotW / 2);
  showTooltipAt(rect.left + (px / width) * rect.width, rect.top + 40, entries, formatTick(t, span));
}

function hideTooltip() {
  els.tooltip.removeAttribute("data-visible");
}

els.chart.addEventListener("mousemove", (event) => {
  if (!crosshairLine) return;
  lastCrosshairX = event.clientX;
  if (crosshairRafPending) return;
  crosshairRafPending = true;
  requestAnimationFrame(() => {
    crosshairRafPending = false;
    const rect = els.chart.getBoundingClientRect();
    const svgX = ((lastCrosshairX - rect.left) / rect.width) * (els.chart.viewBox.baseVal.width || 900);
    crosshairLine.setAttribute("x1", svgX);
    crosshairLine.setAttribute("x2", svgX);
    crosshairLine.classList.add("visible");
    showPointerTooltip({ clientX: lastCrosshairX, clientY: rect.top + rect.height / 2 });
  });
});

els.chart.addEventListener("mouseleave", () => {
  if (crosshairLine) crosshairLine.classList.remove("visible");
  hideTooltip();
});

els.chart.addEventListener("touchstart", (event) => {
  if (!currentCtx || !currentCtx.views.length) return;
  const touch = event.touches[0];
  if (!touch) return;
  event.preventDefault();
  const rect = els.chart.getBoundingClientRect();
  const svgX = ((touch.clientX - rect.left) / rect.width) * (els.chart.viewBox.baseVal.width || 900);
  const margin = { left: 78, right: 24 };
  const width = els.chart.viewBox.baseVal.width || 900;
  const plotW = Math.max(1, width - margin.left - margin.right);
  const span = currentCtx.tMax - currentCtx.tMin;
  const t = span > 0 ? currentCtx.tMin + ((svgX - margin.left) / plotW) * span : currentCtx.tMin;
  const entries = tooltipEntriesAt(t);
  if (!entries.length) return;
  showTooltipAt(touch.clientX, touch.clientY, entries, formatTick(t, span));
  if (crosshairLine) {
    crosshairLine.setAttribute("x1", svgX);
    crosshairLine.setAttribute("x2", svgX);
    crosshairLine.classList.add("visible");
  }
}, { passive: false });

/* ── Legend (click to show/hide a view) ── */

function renderLegend(allViews) {
  const legend = els.legend;
  legend.replaceChildren();
  if (allViews.length < 2) {
    legend.hidden = true;
    return;
  }
  legend.hidden = false;
  const fragment = document.createDocumentFragment();
  for (const view of allViews) {
    const item = document.createElement("li");
    item.className = "legend-item";
    const button = document.createElement("button");
    button.type = "button";
    button.dataset.view = view.key;
    const hidden = state.hiddenViews.has(view.key);
    button.setAttribute("aria-pressed", String(!hidden));
    if (hidden) button.classList.add("is-hidden");

    const swatch = document.createElement("span");
    swatch.className = "legend-swatch";
    swatch.style.background = view.color;
    button.appendChild(swatch);

    const label = document.createElement("span");
    label.className = "legend-label";
    label.textContent = shortViewLabel(view);
    label.title = shortViewLabel(view);
    button.appendChild(label);

    item.appendChild(button);
    fragment.appendChild(item);
  }
  legend.appendChild(fragment);
}

els.legend.addEventListener("click", (event) => {
  const button = event.target.closest("button");
  if (!button || !button.dataset.view) return;
  const key = button.dataset.view;
  if (state.hiddenViews.has(key)) {
    state.hiddenViews.delete(key);
  } else {
    state.hiddenViews.add(key);
  }
  render();
  hideTooltip();
});

/* ── Table ── */

function renderTable(views) {
  els.table.replaceChildren();
  const points = views
    .flatMap((view) => view.points.map((point) => ({ point, view })))
    .sort((a, b) => b.point.time - a.point.time);
  if (!points.length) {
    els.tableEmpty.hidden = false;
    return;
  }
  els.tableEmpty.hidden = true;
  const fragment = document.createDocumentFragment();
  points.forEach(({ point, view }, index) => {
    const tr = document.createElement("tr");
    const runUrl = point.run.run?.url || "";
    const cells = [
      point.generatedAt,
      view, /* rendered specially below */
      (point.run.sha || "").slice(0, 7),
      formatValue(point.value, point.unit),
      runUrl,
    ];
    cells.forEach((cell, cellIndex) => {
      const td = document.createElement("td");
      if (cellIndex === 1) {
        const swatch = document.createElement("span");
        swatch.className = "table-swatch";
        swatch.style.background = view.color;
        td.appendChild(swatch);
        const label = document.createElement("span");
        label.textContent = shortViewLabel(view);
        label.title = shortViewLabel(view);
        td.appendChild(label);
      } else if (cellIndex !== 4 && typeof cell === "string" && cell) {
        td.setAttribute("dir", "auto");
        td.textContent = cell;
      } else if (cellIndex === 4 && cell && isSafeUrl(cell)) {
        const a = document.createElement("a");
        a.href = cell;
        a.target = "_blank";
        a.rel = "noopener";
        a.textContent = "View Run";
        td.appendChild(a);
      } else if (cellIndex === 4 && !cell) {
        td.textContent = "\u2014";
        td.style.color = "var(--muted)";
      } else {
        td.textContent = cell;
      }
      tr.appendChild(td);
    });
    if (!prefersReducedMotionNow()) {
      tr.style.animationDelay = `${Math.min(index * 20, 400)}ms`;
    }
    fragment.appendChild(tr);
  });
  els.table.appendChild(fragment);
}

/* ── Render pipeline ── */

function render() {
  els.rangeOutput.textContent = els.range.value;

  pruneSelection();
  renderChips();

  const { allViews, views, capped } = visibleViews();

  const units = new Set(views.map((view) => view.unit));
  const unitText = views.length
    ? units.size <= 1
      ? `unit: ${views[0].unit || "value"}`
      : "normalized 0–100% (mixed units)"
    : "";
  const hiddenCount = allViews.length - views.length;
  const bits = [
    `${views.length} view${views.length === 1 ? "" : "s"}`,
    unitText,
    capped ? "view cap 24 reached" : "",
    hiddenCount ? `${hiddenCount} hidden` : "",
  ].filter(Boolean);
  els.chartSubtitle.textContent = bits.join(" · ");

  renderChart(views);
  renderLegend(allViews);

  /* Range context: rows shown across views */
  const rowCount = views.reduce((sum, view) => sum + view.points.length, 0);
  els.rangeContext.textContent = rowCount > 0 ? `${rowCount} run${rowCount === 1 ? "" : "s"} across ${views.length} view${views.length === 1 ? "" : "s"}` : "";

  renderTable(views);

  if (els.chartStatus) {
    els.chartStatus.textContent = views.length
      ? `Chart updated: ${views.length} view${views.length === 1 ? "" : "s"} overlaid`
      : "No data matches the current filters. Try toggling different options above.";
  }
}

/* ── RAF-throttled render for range slider ── */

let rafPending = false;
function throttledRender() {
  if (rafPending) return;
  rafPending = true;
  requestAnimationFrame(() => {
    rafPending = false;
    render();
  });
}

/* ── Bootstrap ── */

const FETCH_TIMEOUT_MS = 15_000;

async function main() {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);
  let response;
  try {
    response = await fetch("data/history.json", { cache: "no-store", signal: controller.signal });
  } finally {
    clearTimeout(timeoutId);
  }
  if (response.status === 404) {
    state.history = { runs: [] };
  } else if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  } else {
    state.history = await response.json();
    /* Validate basic shape of the response */
    if (!state.history || typeof state.history !== "object") {
      state.history = { runs: [] };
    }
    if (!Array.isArray(state.history.runs)) {
      state.history.runs = [];
    }
  }
  buildPoints();

  /* Remove spinner from summary */
  const spinner = els.summary.querySelector(".spinner");
  if (spinner) spinner.remove();

  const runCount = (state.history?.runs || []).length;
  const metricCount = state.points.length;
  if (runCount === 0) {
    els.summary.textContent = "No benchmark data available yet. Results will appear once CI runs complete.";
    /* Show first-run welcome section when there's no data */
    if (els.welcome) els.welcome.hidden = false;
  } else {
    /* Hide welcome section once data exists */
    if (els.welcome) els.welcome.hidden = true;
    /* Safe DOM construction — avoids innerHTML with user-derived data */
    els.summary.textContent = "";
    const s1 = document.createElement("strong");
    s1.textContent = runCount;
    els.summary.appendChild(s1);
    els.summary.appendChild(document.createTextNode(" runs, "));
    const s2 = document.createElement("strong");
    s2.textContent = metricCount;
    els.summary.appendChild(s2);
    els.summary.appendChild(document.createTextNode(" metrics. Toggle filter chips to overlay views; use the recent-run slider to zoom."));

    /* Show chart interaction tip on first visit (dismissed per browser) */
    if (els.chartTip && !safeStorage.get("bench-tip-dismissed")) {
      els.chartTip.hidden = false;
    }
  }
  els.summary.classList.remove("summary-error");
  els.summary.classList.add("summary-loaded");

  initSelection();
  render();

  els.range.addEventListener("input", throttledRender);

  els.resetFilters.addEventListener("click", () => {
    state.selection.series = new Set();
    state.selection.suites = new Set();
    state.selection.metrics = new Set();
    state.selection.cases = new Set();
    state.hiddenViews.clear();
    /* Drop URL + stored state so initSelection falls back to defaults;
       the URL stays clean until the user picks something again. */
    window.history.replaceState(null, "", window.location.pathname);
    safeStorage.set(SELECTION_STORAGE_KEY, "");
    els.range.value = "40";
    safeStorage.set(RANGE_STORAGE_KEY, "40");
    initSelection();
    render();
  });

  /* Dismiss chart tip and persist preference */
  if (els.chartTipDismiss) {
    els.chartTipDismiss.addEventListener("click", () => {
      if (els.chartTip) els.chartTip.hidden = true;
      safeStorage.set("bench-tip-dismissed", "1");
    });
  }

  let resizeTimeout;
  window.addEventListener("resize", () => {
    clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(() => {
      renderChart(visibleViews().views);
    }, 100);
  });
}

function renderLoadError(error) {
  const spinner = els.summary.querySelector(".spinner");
  if (spinner) spinner.remove();
  const msg = error.name === "AbortError"
    ? "Loading timed out — the server took too long to respond."
    : `Failed to load benchmark history: ${error.message}`;
  els.summary.textContent = msg;
  els.summary.classList.add("summary-error");
  /* Add retry button so users can recover without refreshing */
  const retryBtn = document.createElement("button");
  retryBtn.textContent = "Retry";
  retryBtn.className = "retry-btn";
  retryBtn.addEventListener("click", () => {
    retryBtn.disabled = true; /* Prevent concurrent retries */
    els.summary.textContent = "";
    els.summary.classList.remove("summary-error");
    const newSpinner = document.createElement("span");
    newSpinner.className = "spinner";
    newSpinner.setAttribute("aria-hidden", "true");
    els.summary.appendChild(newSpinner);
    els.summary.appendChild(document.createTextNode(" Retrying\u2026 this usually takes a few seconds"));
    main().catch(renderLoadError);
  });
  els.summary.appendChild(retryBtn);
}

main().catch(renderLoadError);
