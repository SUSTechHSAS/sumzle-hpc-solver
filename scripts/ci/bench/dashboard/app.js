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

const state = {
  history: { runs: [] },
  points: [],
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
  chart: document.getElementById("chart"),
  chartTitle: document.getElementById("chart-title"),
  chartSubtitle: document.getElementById("chart-subtitle"),
  tooltip: document.getElementById("tooltip"),
  chartStatus: document.getElementById("chart-status"),
  chartTip: document.getElementById("chart-tip"),
  chartTipDismiss: document.getElementById("chart-tip-dismiss"),
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

function setOptions(select, values, selected) {
  select.replaceChildren();
  if (!values.length) {
    /* Placeholder when no data — teaches the interface even when empty */
    const placeholder = document.createElement("option");
    placeholder.disabled = true;
    placeholder.selected = true;
    placeholder.textContent = "No data";
    select.appendChild(placeholder);
    return;
  }
  values.forEach((value) => option(select, value));
  if (selected && values.includes(selected)) {
    select.value = selected;
  }
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
      });
    }
  }
}

function initControls() {
  const params = new URLSearchParams(window.location.search);
  const requestedPr = params.get("pr");
  const requestedSuite = params.get("suite");
  const requestedMetric = params.get("metric");

  const series = [...new Set(state.points.map((point) => point.series))].sort();
  setOptions(els.series, series, requestedPr ? `PR #${requestedPr}` : "main");

  const suites = [...new Set(state.points.map((point) => point.suite))].sort();
  setOptions(els.suite, suites, requestedSuite || "cli");

  refreshMetricOptions(requestedMetric);
}

function filteredBase() {
  return state.points.filter((point) => point.series === els.series.value && point.suite === els.suite.value);
}

function refreshMetricOptions(requestedMetric) {
  const metrics = [...new Set(filteredBase().map((point) => point.metricName))].sort();
  setOptions(els.metric, metrics, requestedMetric && metrics.includes(requestedMetric) ? requestedMetric : metrics[0]);
  refreshCaseOptions();
}

function refreshCaseOptions() {
  const cases = [
    ...new Set(
      filteredBase()
        .filter((point) => point.metricName === els.metric.value)
        .map((point) => point.caseName),
    ),
  ].sort();
  setOptions(els.case, cases, cases[0]);
  render();
}

function allFilteredPoints() {
  return state.points
    .filter(
      (point) =>
        point.series === els.series.value &&
        point.suite === els.suite.value &&
        point.metricName === els.metric.value &&
        point.caseName === els.case.value,
    )
    .sort((a, b) => a.generatedAt.localeCompare(b.generatedAt));
}

function visiblePoints() {
  const points = allFilteredPoints();
  const count = Number(els.range.value);
  return points.slice(Math.max(0, points.length - count));
}

function formatValue(value, unit) {
  if (unit === "ns") {
    if (value >= 1e9) return `${(value / 1e9).toFixed(2)} s`;
    if (value >= 1e6) return `${(value / 1e6).toFixed(1)} ms`;
    if (value >= 1e3) return `${(value / 1e3).toFixed(1)} \u00b5s`;
  }
  return `${Number.isInteger(value) ? value : value.toFixed(2)}${unit ? ` ${unit}` : ""}`;
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

function renderChart(points) {
  const svg = els.chart;
  clearSvg(svg);

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

  if (!points.length) {
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
    emptyHint.textContent = "Try choosing a different branch, suite, or metric";
    fragment.appendChild(emptyHint);

    svg.appendChild(fragment);
    return;
  }

  /* Store point data for event delegation lookups */
  chartPointData = points;

  const values = points.map((point) => point.value);
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
  min -= pad;
  max += pad;

  const x = (index) => margin.left + (points.length === 1 ? plotW / 2 : (index / (points.length - 1)) * plotW);
  const y = (value) => margin.top + plotH - ((value - min) / (max - min)) * plotH;

  /* Grid lines + Y-axis labels */
  for (let i = 0; i <= 4; i += 1) {
    const gy = margin.top + (plotH / 4) * i;
    const gridLine = svgEl("line", { x1: margin.left, x2: width - margin.right, y1: gy, y2: gy });
    gridLine.classList.add("chart-grid-line");
    fragment.appendChild(gridLine);
    const label = svgEl("text", { x: margin.left - 12, y: gy + 4, "text-anchor": "end", "font-size": "12" });
    label.classList.add("chart-axis-label");
    label.textContent = formatValue(max - ((max - min) / 4) * i, points[0].unit);
    fragment.appendChild(label);
  }

  /* Build coordinate pairs for smooth curve */
  const coords = points.map((point, index) => [x(index), y(point.value)]);

  /* Area fill under the curve */
  const areaPathD =
    smoothPath(coords) +
    ` L ${coords[coords.length - 1][0]} ${margin.top + plotH} L ${coords[0][0]} ${margin.top + plotH} Z`;
  const area = svgEl("path", { d: areaPathD, "stroke-width": "0" });
  area.classList.add("chart-area");
  fragment.appendChild(area);

  /* Line path with draw animation */
  const linePathD = smoothPath(coords);
  const line = svgEl("path", { d: linePathD, fill: "none", "stroke-width": "2.5" });
  line.classList.add("chart-line");
  fragment.appendChild(line);

  /* Data point dots with invisible hit areas */
  points.forEach((point, index) => {
    const cx = x(index);
    const cy = y(point.value);

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
      "aria-label": `${point.suite} / ${point.metricName}: ${formatValue(point.value, point.unit)}`,
      "aria-describedby": "tooltip",
      "data-point": index,
    });
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

  /* X-axis labels — with collision avoidance for dense datasets */
  const labelSkip = Math.max(1, Math.ceil(points.length / 8));
  points.forEach((point, index) => {
    if (index % labelSkip !== 0 && index !== points.length - 1) return;
    const label = svgEl("text", {
      x: x(index),
      y: height - 24,
      "text-anchor": "middle",
      "font-size": "12",
      "overflow": "hidden",
    });
    label.classList.add("chart-axis-label");
    label.textContent = (point.run.sha || "").slice(0, 7);
    fragment.appendChild(label);
  });

  /* Batch DOM insert — single reflow instead of N */
  svg.appendChild(fragment);

  /* Post-append: line draw animation requires element in DOM for getTotalLength() */
  if (!prefersReducedMotionNow()) {
    const lineLength = line.getTotalLength();
    line.style.strokeDasharray = lineLength;
    line.style.strokeDashoffset = lineLength;
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
  const point = getPointFromTarget(event.target);
  if (point) showTooltip(event, point);
});

els.chart.addEventListener("mousemove", (event) => {
  const point = getPointFromTarget(event.target);
  if (point) showTooltip(event, point);
});

els.chart.addEventListener("mouseout", (event) => {
  const point = getPointFromTarget(event.target);
  if (point) {
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
  const point = getPointFromTarget(target);
  if (point) showTooltip(event, point);
}, { passive: false });

els.chart.addEventListener("focusin", (event) => {
  if (!event.target.hasAttribute("data-point")) return;
  const point = getPointFromTarget(event.target);
  if (point) showTooltip(event, point);
});

els.chart.addEventListener("focusout", (event) => {
  if (!event.target.hasAttribute("data-point")) return;
  hideTooltip();
});

/* ── Tooltip ── */

function showTooltip(event, point) {
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
  titleDiv.textContent = `${point.series}  ${(point.run.sha || "").slice(0, 7)}`;
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
  if (tx > maxX) tx = Math.max(0, maxX);
  if (tooltipRect.bottom - rect.top > rect.height) {
    ty = rect.height - tooltipRect.height - 8;
  }

  els.tooltip.style.setProperty("--tx", `${tx}px`);
  els.tooltip.style.setProperty("--ty", `${ty}px`);
}

function hideTooltip() {
  els.tooltip.removeAttribute("data-visible");
}

/* ── Table ── */

function renderTable(points) {
  els.table.replaceChildren();
  if (!points.length) {
    els.tableEmpty.hidden = false;
    return;
  }
  els.tableEmpty.hidden = true;
  const fragment = document.createDocumentFragment();
  [...points].reverse().forEach((point, index) => {
    const tr = document.createElement("tr");
    const runUrl = point.run.run?.url || "";
    const cells = [
      point.generatedAt,
      point.series,
      (point.run.sha || "").slice(0, 7),
      formatValue(point.value, point.unit),
      runUrl,
    ];
    cells.forEach((cell, cellIndex) => {
      const td = document.createElement("td");
      /* Add dir="auto" to user-generated content cells for RTL support */
      if (cellIndex !== 4 && typeof cell === "string" && cell) {
        td.setAttribute("dir", "auto");
      }
      if (cellIndex === 4 && cell && isSafeUrl(cell)) {
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
  const points = visiblePoints();
  const allForFilter = allFilteredPoints();

  /* Chart title/subtitle with unit context */
  const subtitle = points.length
    ? `${els.suite.value} / ${els.metric.value}  \u00B7  ${els.case.value}  \u00B7  ${points[0].unit || "value"}`
    : `${els.suite.value} / ${els.metric.value}  \u00B7  ${els.case.value}`;
  els.chartSubtitle.textContent = subtitle;

  /* Range context: showing X of Y total */
  const total = allForFilter.length;
  const showing = points.length;
  els.rangeContext.textContent = total > 0 ? `Showing ${showing} of ${total} runs` : "";

  renderChart(points);
  renderTable(points);

  /* Announce chart update to screen readers */
  if (els.chartStatus) {
    els.chartStatus.textContent = points.length
      ? `Chart updated: ${showing} of ${total} ${els.metric.value} data points for ${els.series.value}, ${els.suite.value}`
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
    els.summary.appendChild(document.createTextNode(" metrics. Use filters and the recent-run slider to zoom."));

    /* Show chart interaction tip on first visit (dismissed per browser) */
    if (els.chartTip && !localStorage.getItem("bench-tip-dismissed")) {
      els.chartTip.hidden = false;
    }
  }
  els.summary.classList.remove("summary-error");
  els.summary.classList.add("summary-loaded");
  initControls();

  els.series.addEventListener("change", () => {
    refreshMetricOptions();
  });
  els.suite.addEventListener("change", () => {
    refreshMetricOptions();
  });
  els.metric.addEventListener("change", refreshCaseOptions);
  els.case.addEventListener("change", render);
  els.range.addEventListener("input", throttledRender);

  /* Dismiss chart tip and persist preference */
  if (els.chartTipDismiss) {
    els.chartTipDismiss.addEventListener("click", () => {
      if (els.chartTip) els.chartTip.hidden = true;
      localStorage.setItem("bench-tip-dismissed", "1");
    });
  }

  let resizeTimeout;
  window.addEventListener("resize", () => {
    clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(() => {
      renderChart(visiblePoints());
    }, 100);
  });
}

main().catch((error) => {
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
    main().catch((retryError) => {
      /* Re-show error so user can retry again */
      const spin = els.summary.querySelector(".spinner");
      if (spin) spin.remove();
      const msg2 = retryError.name === "AbortError"
        ? "Loading timed out \u2014 the server took too long to respond."
        : `Failed to load benchmark history: ${retryError.message}`;
      els.summary.textContent = msg2;
      els.summary.classList.add("summary-error");
      const retryBtn2 = document.createElement("button");
      retryBtn2.textContent = "Retry";
      retryBtn2.className = "retry-btn";
      els.summary.appendChild(retryBtn2);
    });
  });
  els.summary.appendChild(retryBtn);
});
