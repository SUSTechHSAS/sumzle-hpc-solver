/* ── Touch/device detection for contextual copy ── */
if (!window.matchMedia("(hover: hover)").matches) {
  const tipText = document.getElementById("chart-tip-text");
  if (tipText) tipText.textContent = "Tap data points for details — use arrow keys to navigate the chart";
}

const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
const prefersReducedMotionNow = () => prefersReducedMotion.matches;
const canHover = window.matchMedia("(hover: hover)").matches;
// Listen for runtime changes to OS accessibility setting
prefersReducedMotion.addEventListener("change", () => {
  // CSS handles the visual reset; this listener ensures
  // future JS animation decisions reflect the current state.
});

/* ── Constants ── */
const PALETTE_SIZE = 12;
const MAX_VIEWS = 24;
const SELECTION_STORAGE_KEY = "bench-dash-selection";
const RANGE_STORAGE_KEY = "bench-dash-range";

const state = {
  history: { runs: [] },
  points: [],
  /* One set per filter; values are kept when they stay valid, so changing
     one filter never resets the others (issue #44). */
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
  series: document.getElementById("series-filter"),
  suite: document.getElementById("suite-filter"),
  metric: document.getElementById("metric-filter"),
  case: document.getElementById("case-filter"),
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

function metricKey(metric) {
  return `${metric.suite}::${metric.metric}::${caseName(metric)}`;
}

function option(select, value, label = value) {
  const node = document.createElement("option");
  node.value = value;
  node.textContent = label;
  select.appendChild(node);
}

function pointTime(point) {
  const t = Date.parse(point.generatedAt);
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
        key: metricKey(metric),
        value,
        unit: metric.unit,
        generatedAt: run.generated_at || "",
        time: pointTime({ generatedAt: run.generated_at || "" }),
      });
    }
  }
}

function sortedUnique(values) {
  return [...new Set(values)].sort();
}

/* ── Filter option sets ──
   series and suite options are global (as before); metric options are the
   union of metric names for the selected series + suites, and case options
   depend on the selected series + suites + metrics. */

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

/* Keep every still-valid selection; only fall back to the first available
   option when nothing of the previous selection remains (issue #44: changing
   one filter must not reset unrelated filters back to their defaults). */
function prune(selection, valid) {
  const kept = new Set([...selection].filter((value) => valid.includes(value)));
  if (!kept.size && valid.length) kept.add(valid[0]);
  return kept;
}

function pruneSelection() {
  const sel = state.selection;
  sel.metrics = prune(sel.metrics, validMetrics());
  sel.cases = prune(sel.cases, validCases());
}

function renderSelect(select, values, selection) {
  if (!values.length) {
    /* Placeholder when no data — teaches the interface even when empty */
    select.replaceChildren();
    const placeholder = document.createElement("option");
    placeholder.disabled = true;
    placeholder.selected = true;
    placeholder.textContent = "No data";
    select.appendChild(placeholder);
    return;
  }
  const current = Array.from(select.options).map((node) => node.value);
  const sameList =
    current.length === values.length && current.every((value, index) => value === values[index]);
  if (!sameList) {
    select.replaceChildren();
    values.forEach((value) => option(select, value));
  }
  for (const node of select.options) node.selected = selection.has(node.value);
  select.size = Math.max(1, Math.min(values.length, 8));
}

function selectedValues(select) {
  return Array.from(select.selectedOptions).map((node) => node.value);
}

function readUiSelection() {
  return {
    series: new Set(selectedValues(els.series)),
    suites: new Set(selectedValues(els.suite)),
    metrics: new Set(selectedValues(els.metric)),
    cases: new Set(selectedValues(els.case)),
  };
}

function refreshOptionLists() {
  const sel = state.selection;
  renderSelect(els.series, allSeries(), sel.series);
  renderSelect(els.suite, allSuites(), sel.suites);
  renderSelect(els.metric, validMetrics(), sel.metrics);
  renderSelect(els.case, validCases(), sel.cases);
}

/* ── Selection persistence (URL + localStorage) ──
   "|" is the separator because case names legitimately contain ", ". */

function selectionToObject() {
  const sel = state.selection;
  return {
    series: [...sel.series],
    suites: [...sel.suites],
    metrics: [...sel.metrics],
    cases: [...sel.cases],
  };
}

function persistSelection() {
  const params = new URLSearchParams();
  const join = (values) => [...values].sort().join("|");
  params.set("series", join(state.selection.series));
  params.set("suite", join(state.selection.suites));
  params.set("metric", join(state.selection.metrics));
  params.set("case", join(state.selection.cases));
  const qs = params.toString();
  window.history.replaceState(null, "", qs ? `?${qs}` : window.location.pathname);
  safeStorage.set(SELECTION_STORAGE_KEY, JSON.stringify(selectionToObject()));
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

  const seriesParam = paramValues(params, "series");
  const legacyPr = params.get("pr");
  const suitesParam = paramValues(params, "suite");
  const metricsParam = paramValues(params, "metric");
  const casesParam = paramValues(params, "case");

  const seriesRequested = seriesParam || (legacyPr ? [`PR #${legacyPr}`] : null) || (stored && stored.series);
  const suitesRequested = suitesParam || (stored && stored.suites);
  const metricsRequested = metricsParam || (stored && stored.metrics);
  const casesRequested = casesParam || (stored && stored.cases);

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
   A view is one (series, suite, metric, case) combination. Selected filters
   are overlaid in the same chart (issue #46). */

function viewKey(series, suite, metric, caseValue) {
  return `${series}::${suite}::${metric}::${caseValue}`;
}

function buildViews() {
  const sel = state.selection;
  const recent = Math.max(1, Number(els.range.value) || 40);
  const views = [];
  for (const series of [...sel.series].sort()) {
    for (const suite of [...sel.suites].sort()) {
      for (const metric of [...sel.metrics].sort()) {
        for (const caseValue of [...sel.cases].sort()) {
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
          views.push({
            key: viewKey(series, suite, metric, caseValue),
            series,
            suite,
            metric,
            caseName: caseValue,
            unit: points[0].unit,
            points,
          });
          if (views.length >= MAX_VIEWS) return views;
        }
      }
    }
  }
  return views;
}

function visibleViews() {
  const allViews = buildViews();
  return { allViews, views: allViews.filter((view) => !state.hiddenViews.has(view.key)) };
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

function computeContext() {
  const { allViews, views } = visibleViews();
  const unitUniform = new Set(views.map((view) => view.unit)).size <= 1;
  const allPoints = views.flatMap((view) => view.points);
  const ctx = {
    allViews,
    views,
    unitUniform,
    unit: unitUniform && views.length ? views[0].unit : "",
    tMin: 0,
    tMax: 0,
    absMin: 0,
    absMax: 0,
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

/* ── Point data store for event delegation ── */

let chartPointData = [];

/* ── Chart rendering ── */

let crosshairLine = null;

function renderChart(ctx) {
  const svg = els.chart;
  clearSvg(svg);
  chartPointData = [];

  /* Read dimensions once, then batch all DOM writes via fragment */
  const width = svg.clientWidth || 900;
  const height = svg.clientHeight || 420;
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);

  const margin = { top: 24, right: 24, bottom: 64, left: 78 };
  const plotW = Math.max(1, width - margin.left - margin.right);
  const plotH = Math.max(1, height - margin.top - margin.bottom);

  const fragment = document.createDocumentFragment();

  /* Crosshair guide line */
  crosshairLine = svgEl("line", {
    x1: 0,
    x2: 0,
    y1: margin.top,
    y2: height - margin.bottom,
    class: "chart-crosshair",
  });
  fragment.appendChild(crosshairLine);

  const views = ctx.views;

  if (!views.length) {
    /* Visual empty state: dashed rectangle + structured text */
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
    emptyHint.textContent = "Try choosing different branches, suites, metrics, or parameter sets";
    fragment.appendChild(emptyHint);

    svg.appendChild(fragment);
    return;
  }

  const span = ctx.tMax - ctx.tMin;
  const x = (t) => margin.left + (span > 0 ? ((t - ctx.tMin) / span) * plotW : plotW / 2);
  const y = (value, scale) =>
    margin.top + plotH - ((value - scale.min) / Math.max(1e-9, scale.max - scale.min)) * plotH;

  /* Grid lines + Y-axis labels (absolute values, or % when units are mixed) */
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
      x: span > 0 ? x(t) : width / 2,
      y: height - 24,
      "text-anchor": "middle",
      "font-size": "12",
    });
    label.classList.add("chart-axis-label");
    label.textContent = formatTick(t, span);
    fragment.appendChild(label);
  }

  const drawArea = views.length <= 3;

  views.forEach((view, viewIndex) => {
    const colorIndex = ctx.allViews.indexOf(view) % PALETTE_SIZE;
    const colorVar = `var(--view-${colorIndex})`;
    const scale = ctx.unitUniform ? { min: ctx.absMin, max: ctx.absMax } : viewScale(view);
    const coords = view.points.map((point) => [x(point.time), y(point.value, scale)]);

    /* Area fill under the curve — only for a few views to avoid mud */
    if (drawArea) {
      const areaPathD = coords.length === 1
        ? `M ${coords[0][0]} ${coords[0][1]} L ${coords[0][0]} ${margin.top + plotH} Z`
        : smoothPath(coords) +
          ` L ${coords[coords.length - 1][0]} ${margin.top + plotH} L ${coords[0][0]} ${margin.top + plotH} Z`;
      const area = svgEl("path", { d: areaPathD, "stroke-width": "0" });
      area.classList.add("chart-area");
      area.style.setProperty("--view-color", colorVar);
      fragment.appendChild(area);
    }

    /* Line path with draw animation */
    const linePathD = smoothPath(coords);
    if (linePathD) {
      const line = svgEl("path", { d: linePathD, fill: "none", "stroke-width": "2.5" });
      line.classList.add("chart-line");
      line.style.setProperty("--view-color", colorVar);
      fragment.appendChild(line);
    }

    /* Data point dots with invisible hit areas */
    view.points.forEach((point) => {
      const index = chartPointData.length;
      chartPointData.push({ point, view, colorIndex });
      const cx = x(point.time);
      const cy = y(point.value, scale);

      /* Invisible hit area for touch (44×44 → r=22) */
      const hit = svgEl("circle", {
        cx,
        cy,
        r: "22",
        class: "chart-hit",
        "data-hit": index,
      });
      fragment.appendChild(hit);

      /* Visible dot */
      const circle = svgEl("circle", {
        cx,
        cy,
        r: "4",
        tabindex: "0",
        role: "graphics-symbol",
        "aria-label": `${view.series} / ${view.suite}/${view.metric} / ${view.caseName}: ${formatValue(point.value, point.unit)}`,
        "aria-describedby": "tooltip",
        "data-point": index,
      });
      circle.style.setProperty("--view-color", colorVar);
      /* Staggered entrance animation */
      if (!prefersReducedMotionNow()) {
        circle.style.animationDelay = `${Math.min(index * 25, 600)}ms`;
      }

      /* Keyboard navigation stays on individual circles (a11y requirement) */
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
      fragment.appendChild(circle);
    });
  });

  /* Batch DOM insert — single reflow instead of N */
  svg.appendChild(fragment);

  /* Post-append: line draw animation requires element in DOM for getTotalLength() */
  if (!prefersReducedMotionNow()) {
    svg.querySelectorAll("path.chart-line").forEach((line) => {
      const lineLength = line.getTotalLength();
      line.style.strokeDasharray = lineLength;
      line.style.strokeDashoffset = lineLength;
    });
  }
}

/* ── Crosshair tracking (RAF-throttled to avoid jank) ── */

let crosshairRafPending = false;
let lastCrosshairX = 0;

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
  });
});

els.chart.addEventListener("mouseleave", () => {
  if (crosshairLine) crosshairLine.classList.remove("visible");
});

/* ── Delegated chart events (hover/touch/focus) ── */

function getPointFromTarget(target) {
  const idx = target.getAttribute("data-hit") || target.getAttribute("data-point");
  if (idx == null) return null;
  return chartPointData[Number(idx)] || null;
}

els.chart.addEventListener("mouseover", (event) => {
  const entry = getPointFromTarget(event.target);
  if (entry) showTooltip(event, entry);
});

els.chart.addEventListener("mousemove", (event) => {
  const entry = getPointFromTarget(event.target);
  if (entry) showTooltip(event, entry);
});

els.chart.addEventListener("mouseout", (event) => {
  const entry = getPointFromTarget(event.target);
  if (entry) {
    /* Only hide if we're leaving the hit area/dot entirely */
    const related = event.relatedTarget;
    if (!related || !related.closest("[data-hit], [data-point]")) {
      hideTooltip();
    }
  }
});

els.chart.addEventListener("touchstart", (event) => {
  const target = event.target.closest("[data-hit], [data-point]");
  if (!target) return;
  event.preventDefault();
  const entry = getPointFromTarget(target);
  if (entry) showTooltip(event, entry);
}, { passive: false });

els.chart.addEventListener("focusin", (event) => {
  if (!event.target.hasAttribute("data-point")) return;
  const entry = getPointFromTarget(event.target);
  if (entry) showTooltip(event, entry);
});

els.chart.addEventListener("focusout", (event) => {
  if (!event.target.hasAttribute("data-point")) return;
  hideTooltip();
});

/* ── Tooltip ── */

function showTooltip(event, entry) {
  const point = entry.point;
  const rect = els.chart.parentElement.getBoundingClientRect();
  let xCoord;
  let yCoord;
  const touch = event.touches?.[0];
  if (touch) {
    xCoord = touch.clientX - rect.left;
    yCoord = touch.clientY - rect.top;
  } else if (event.clientX !== undefined && event.clientY !== undefined) {
    xCoord = event.clientX - rect.left;
    yCoord = event.clientY - rect.top;
  } else {
    const targetRect = event.target.getBoundingClientRect();
    xCoord = targetRect.left - rect.left + targetRect.width / 2;
    yCoord = targetRect.top - rect.top - 12;
  }

  /* GPU-accelerated positioning via CSS custom properties */
  let tx = xCoord + 18;
  let ty = yCoord + 18;

  /* Build tooltip content first so dimensions reflect actual content */
  els.tooltip.textContent = "";

  const titleDiv = document.createElement("div");
  titleDiv.className = "tooltip-title";
  titleDiv.setAttribute("dir", "auto");
  const swatch = document.createElement("span");
  swatch.className = "tooltip-swatch";
  swatch.setAttribute("aria-hidden", "true");
  swatch.style.setProperty("--view-color", `var(--view-${entry.colorIndex})`);
  titleDiv.appendChild(swatch);
  titleDiv.appendChild(document.createTextNode(`${point.series}  ${(point.run.sha || "").slice(0, 7)}`));
  els.tooltip.appendChild(titleDiv);

  const bodyDiv = document.createElement("div");
  bodyDiv.className = "tooltip-body";
  bodyDiv.setAttribute("dir", "auto");
  bodyDiv.textContent = `${point.suite} / ${point.metricName}  \u00B7  ${point.caseName}`;
  els.tooltip.appendChild(bodyDiv);

  const valueDiv = document.createElement("div");
  valueDiv.className = "tooltip-value";
  valueDiv.textContent = formatValue(point.value, point.unit);
  els.tooltip.appendChild(valueDiv);

  const metaDiv = document.createElement("div");
  metaDiv.className = "tooltip-meta";
  metaDiv.textContent = point.generatedAt;
  els.tooltip.appendChild(metaDiv);

  /* Now show and measure with correct dimensions */
  els.tooltip.setAttribute("data-visible", "");
  const tooltipRect = els.tooltip.getBoundingClientRect();
  const maxX = rect.width - tooltipRect.width - 8;
  const maxY = rect.height - tooltipRect.height - 8;
  tx = Math.min(Math.max(0, tx), Math.max(0, maxX));
  ty = Math.min(Math.max(0, ty), Math.max(0, maxY));

  els.tooltip.style.setProperty("--tx", `${tx}px`);
  els.tooltip.style.setProperty("--ty", `${ty}px`);
}

function hideTooltip() {
  els.tooltip.removeAttribute("data-visible");
}

/* ── Legend ── */

function renderLegend(ctx) {
  els.legend.replaceChildren();
  const { allViews } = ctx;
  if (!allViews.length) {
    els.legend.hidden = true;
    return;
  }
  els.legend.hidden = false;
  const fragment = document.createDocumentFragment();
  allViews.forEach((view, index) => {
    const li = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "legend-item";
    button.style.setProperty("--view-color", `var(--view-${index % PALETTE_SIZE})`);
    const visible = !state.hiddenViews.has(view.key);
    button.setAttribute("aria-pressed", String(visible));
    if (!visible) button.classList.add("is-hidden");
    const label = `${view.series} \u00B7 ${view.suite}/${view.metric} \u00B7 ${view.caseName}`;
    button.title = label;
    button.setAttribute("aria-label", `${label} — ${visible ? "hide" : "show"} view`);

    const swatch = document.createElement("span");
    swatch.className = "legend-swatch";
    swatch.setAttribute("aria-hidden", "true");

    const text = document.createElement("span");
    text.className = "legend-label";
    text.textContent = label;

    const unit = document.createElement("span");
    unit.className = "legend-unit";
    unit.textContent = view.unit || "value";

    button.append(swatch, text, unit);
    button.addEventListener("click", () => {
      if (state.hiddenViews.has(view.key)) {
        state.hiddenViews.delete(view.key);
      } else {
        state.hiddenViews.add(view.key);
      }
      render();
    });
    li.appendChild(button);
    fragment.appendChild(li);
  });
  els.legend.appendChild(fragment);
}

/* ── Table ── */

function renderTable(ctx) {
  els.table.replaceChildren();
  const rows = ctx.views.flatMap((view) =>
    view.points.map((point) => ({ point, view })),
  );
  rows.sort((a, b) => b.point.time - a.point.time);
  if (!rows.length) {
    els.tableEmpty.hidden = false;
    return;
  }
  els.tableEmpty.hidden = true;
  const fragment = document.createDocumentFragment();
  rows.forEach(({ point, view }, index) => {
    const tr = document.createElement("tr");
    const runUrl = point.run.run?.url || "";
    const cells = [
      point.generatedAt,
      `${view.suite} / ${view.metric} \u00B7 ${view.caseName}`,
      point.series,
      (point.run.sha || "").slice(0, 7),
      formatValue(point.value, point.unit),
      runUrl,
    ];
    cells.forEach((cell, cellIndex) => {
      const td = document.createElement("td");
      /* Add dir="auto" to user-generated content cells for RTL support */
      if (cellIndex !== 5 && typeof cell === "string" && cell) {
        td.setAttribute("dir", "auto");
      }
      if (cellIndex === 5 && cell && isSafeUrl(cell)) {
        const a = document.createElement("a");
        a.href = cell;
        a.target = "_blank";
        a.rel = "noopener";
        a.textContent = "View Run";
        td.appendChild(a);
      } else if (cellIndex === 5 && !cell) {
        td.textContent = "\u2014";
        td.style.color = "var(--muted)";
      } else {
        td.textContent = cell;
      }
      tr.appendChild(td);
    });
    /* Staggered row entrance */
    if (!prefersReducedMotionNow()) {
      tr.style.animationDelay = `${Math.min(index * 20, 400)}ms`;
    }
    fragment.appendChild(tr);
  });
  /* Batch DOM insert — single reflow for all rows */
  els.table.appendChild(fragment);
}

/* ── Render pipeline ── */

function render() {
  els.rangeOutput.textContent = els.range.value;
  const ctx = computeContext();

  /* Chart title/subtitle with unit context */
  const { views } = ctx;
  if (!views.length) {
    els.chartSubtitle.textContent = "No data for the selected filters";
  } else if (views.length === 1) {
    const view = views[0];
    els.chartSubtitle.textContent = `${view.suite} / ${view.metric}  \u00B7  ${view.caseName}  \u00B7  ${view.unit || "value"}`;
  } else {
    els.chartSubtitle.textContent = `${views.length} views  \u00B7  ${ctx.unitUniform ? ctx.unit || "value" : "normalized (mixed units)"}`;
  }

  /* Range context: recent N runs per view, plus the view cap */
  let rangeText = views.length
    ? `Showing last ${els.range.value} runs per view \u00B7 ${views.length} view${views.length > 1 ? "s" : ""}`
    : "";
  if (ctx.allViews.length >= MAX_VIEWS) {
    rangeText += ` \u00B7 capped at ${MAX_VIEWS} views`;
  }
  els.rangeContext.textContent = rangeText;

  renderChart(ctx);
  renderTable(ctx);
  renderLegend(ctx);

  /* Announce chart update to screen readers */
  if (els.chartStatus) {
    els.chartStatus.textContent = views.length
      ? `Chart updated: ${views.length} ${views.length === 1 ? "view" : "views"} over ${ctx.allViews.length} selected view${ctx.allViews.length === 1 ? "" : "s"}`
      : "No data matches the current filters. Try selecting different options above.";
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

/* ── Selection change handling ── */

function onSelectionChange() {
  state.selection = readUiSelection();
  /* Keep valid selections; only reset what the new scope no longer offers */
  pruneSelection();
  refreshOptionLists();
  persistSelection();
  render();
}

/* Click-to-toggle on desktop: multi-selects become checkbox lists, so users
   can overlay views without modifier keys. Touch devices keep the native
   picker behavior (change events still flow through onSelectionChange). */
function enableToggleSelect(select) {
  if (canHover) {
    select.addEventListener("mousedown", (event) => {
      const optionEl = event.target instanceof HTMLOptionElement ? event.target : null;
      if (!optionEl || optionEl.disabled) return;
      event.preventDefault();
      optionEl.selected = !optionEl.selected;
      onSelectionChange();
      select.focus();
    });
  }
  select.addEventListener("change", onSelectionChange);
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
    els.summary.appendChild(document.createTextNode(" metrics. Select multiple filters to overlay views in one chart."));

    /* Show chart interaction tip on first visit (dismissed per browser) */
    if (els.chartTip && !safeStorage.get("bench-tip-dismissed")) {
      els.chartTip.hidden = false;
    }
  }
  els.summary.classList.remove("summary-error");
  els.summary.classList.add("summary-loaded");

  initSelection();
  refreshOptionLists();
  render();
  persistSelection();

  enableToggleSelect(els.series);
  enableToggleSelect(els.suite);
  enableToggleSelect(els.metric);
  enableToggleSelect(els.case);

  els.range.addEventListener("input", () => {
    safeStorage.set(RANGE_STORAGE_KEY, els.range.value);
    throttledRender();
  });

  if (els.resetFilters) {
    els.resetFilters.addEventListener("click", () => {
      const seriesAll = allSeries();
      const suitesAll = allSuites();
      state.selection.series = new Set(seriesAll.includes("main") ? ["main"] : seriesAll.slice(0, 1));
      state.selection.suites = new Set(suitesAll.includes("cli") ? ["cli"] : suitesAll.slice(0, 1));
      state.selection.metrics = new Set(validMetrics().slice(0, 1));
      state.selection.cases = new Set(validCases().slice(0, 1));
      state.hiddenViews.clear();
      els.range.value = "40";
      safeStorage.set(RANGE_STORAGE_KEY, "40");
      refreshOptionLists();
      persistSelection();
      render();
    });
  }

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
      renderChart(computeContext());
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
