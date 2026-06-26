const state = {
  history: { runs: [] },
  points: [],
};

const els = {
  summary: document.getElementById("summary"),
  series: document.getElementById("series-filter"),
  suite: document.getElementById("suite-filter"),
  metric: document.getElementById("metric-filter"),
  case: document.getElementById("case-filter"),
  range: document.getElementById("range-filter"),
  rangeOutput: document.getElementById("range-output"),
  chart: document.getElementById("chart"),
  tooltip: document.getElementById("tooltip"),
  table: document.getElementById("run-table"),
};

function seriesName(run) {
  if (run.pull_request && run.pull_request.number) {
    return `PR #${run.pull_request.number}`;
  }
  return run.branch || "main";
}

function caseName(metric) {
  const params = metric.params || {};
  const parts = Object.keys(params)
    .sort()
    .map((key) => `${key}=${params[key]}`);
  return parts.length ? parts.join(", ") : "default";
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
  values.forEach((value) => option(select, value));
  if (selected && values.includes(selected)) {
    select.value = selected;
  }
}

function buildPoints() {
  state.points = [];
  for (const run of state.history.runs || []) {
    for (const metric of run.metrics || []) {
      state.points.push({
        run,
        metric,
        series: seriesName(run),
        suite: metric.suite,
        metricName: metric.metric,
        caseName: caseName(metric),
        key: metricKey(metric),
        value: Number(metric.value),
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

function visiblePoints() {
  const points = state.points
    .filter(
      (point) =>
        point.series === els.series.value &&
        point.suite === els.suite.value &&
        point.metricName === els.metric.value &&
        point.caseName === els.case.value,
    )
    .sort((a, b) => a.generatedAt.localeCompare(b.generatedAt));
  const count = Number(els.range.value);
  return points.slice(Math.max(0, points.length - count));
}

function formatValue(value, unit) {
  if (unit === "ns") {
    if (value >= 1e9) return `${(value / 1e9).toFixed(2)} s`;
    if (value >= 1e6) return `${(value / 1e6).toFixed(1)} ms`;
    if (value >= 1e3) return `${(value / 1e3).toFixed(1)} us`;
  }
  return `${Number.isInteger(value) ? value : value.toFixed(2)} ${unit}`;
}

function clearSvg(svg) {
  while (svg.firstChild) svg.removeChild(svg.firstChild);
}

function svgEl(name, attrs = {}) {
  const node = document.createElementNS("http://www.w3.org/2000/svg", name);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  return node;
}

function renderChart(points) {
  const svg = els.chart;
  clearSvg(svg);

  const width = svg.clientWidth || 900;
  const height = svg.clientHeight || 420;
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);

  const margin = { top: 24, right: 24, bottom: 64, left: 78 };
  const plotW = Math.max(1, width - margin.left - margin.right);
  const plotH = Math.max(1, height - margin.top - margin.bottom);

  if (!points.length) {
    const text = svgEl("text", { x: width / 2, y: height / 2, "text-anchor": "middle", fill: "#5c6670" });
    text.textContent = "No benchmark data for the selected filters";
    svg.appendChild(text);
    return;
  }

  const values = points.map((point) => point.value);
  let min = Math.min(...values);
  let max = Math.max(...values);
  if (min === max) {
    min *= 0.95;
    max *= 1.05;
    if (min === max) max = min + 1;
  }
  const pad = (max - min) * 0.08;
  min -= pad;
  max += pad;

  const x = (index) => margin.left + (points.length === 1 ? plotW / 2 : (index / (points.length - 1)) * plotW);
  const y = (value) => margin.top + plotH - ((value - min) / (max - min)) * plotH;

  for (let i = 0; i <= 4; i += 1) {
    const gy = margin.top + (plotH / 4) * i;
    svg.appendChild(svgEl("line", { x1: margin.left, x2: width - margin.right, y1: gy, y2: gy, stroke: "#e7eaee" }));
    const label = svgEl("text", { x: margin.left - 10, y: gy + 4, "text-anchor": "end", fill: "#5c6670", "font-size": "12" });
    label.textContent = formatValue(max - ((max - min) / 4) * i, points[0].unit);
    svg.appendChild(label);
  }

  const path = points.map((point, index) => `${index === 0 ? "M" : "L"} ${x(index)} ${y(point.value)}`).join(" ");
  svg.appendChild(svgEl("path", { d: path, fill: "none", stroke: "#0a7a6f", "stroke-width": "2.5" }));

  points.forEach((point, index) => {
    const circle = svgEl("circle", { cx: x(index), cy: y(point.value), r: "4", fill: "#8f3f71", tabindex: "0" });
    circle.addEventListener("mouseenter", (event) => showTooltip(event, point));
    circle.addEventListener("mousemove", (event) => showTooltip(event, point));
    circle.addEventListener("mouseleave", hideTooltip);
    circle.addEventListener("focus", (event) => showTooltip(event, point));
    circle.addEventListener("blur", hideTooltip);
    svg.appendChild(circle);
  });

  points.forEach((point, index) => {
    if (index % Math.ceil(points.length / 8) !== 0 && index !== points.length - 1) return;
    const label = svgEl("text", {
      x: x(index),
      y: height - 24,
      "text-anchor": "middle",
      fill: "#5c6670",
      "font-size": "11",
    });
    label.textContent = (point.run.sha || "").slice(0, 7);
    svg.appendChild(label);
  });
}

function showTooltip(event, point) {
  els.tooltip.hidden = false;
  const rect = els.chart.parentElement.getBoundingClientRect();
  let xCoord;
  let yCoord;
  if (event.clientX !== undefined && event.clientY !== undefined) {
    xCoord = event.clientX - rect.left;
    yCoord = event.clientY - rect.top;
  } else {
    const targetRect = event.target.getBoundingClientRect();
    xCoord = targetRect.left - rect.left + targetRect.width / 2;
    yCoord = targetRect.top - rect.top - 10;
  }
  els.tooltip.style.left = `${xCoord + 18}px`;
  els.tooltip.style.top = `${yCoord + 18}px`;
  els.tooltip.textContent = "";
  const lines = [
    `${point.series} ${(point.run.sha || "").slice(0, 7)}`,
    `${point.suite} / ${point.metricName}`,
    point.caseName,
    formatValue(point.value, point.unit),
    point.generatedAt,
  ];
  for (const line of lines) {
    const div = document.createElement("div");
    div.textContent = line;
    els.tooltip.appendChild(div);
  }
}

function hideTooltip() {
  els.tooltip.hidden = true;
}

function renderTable(points) {
  els.table.replaceChildren();
  [...points].reverse().forEach((point) => {
    const tr = document.createElement("tr");
    const runUrl = point.run.run && point.run.run.url ? point.run.run.url : "";
    const cells = [
      point.generatedAt,
      point.series,
      (point.run.sha || "").slice(0, 7),
      formatValue(point.value, point.unit),
      runUrl,
    ];
    cells.forEach((cell, index) => {
      const td = document.createElement("td");
      if (index === 4 && cell) {
        const a = document.createElement("a");
        a.href = cell;
        a.textContent = "workflow";
        td.appendChild(a);
      } else {
        td.textContent = cell;
      }
      tr.appendChild(td);
    });
    els.table.appendChild(tr);
  });
}

function render() {
  els.rangeOutput.textContent = els.range.value;
  const points = visiblePoints();
  renderChart(points);
  renderTable(points);
}

async function main() {
  const response = await fetch("data/history.json", { cache: "no-store" }).catch(() => null);
  if (response && response.status === 404) {
    state.history = { runs: [] };
  } else if (response && !response.ok) {
    throw new Error(`HTTP error while loading history: ${response.status}`);
  } else if (response) {
    state.history = await response.json();
  } else {
    state.history = { runs: [] };
  }
  buildPoints();
  const runCount = (state.history.runs || []).length;
  const metricCount = state.points.length;
  els.summary.textContent = `${runCount} runs, ${metricCount} metrics. Use filters and the recent-run slider to zoom.`;
  initControls();

  els.series.addEventListener("change", () => {
    refreshMetricOptions();
  });
  els.suite.addEventListener("change", () => {
    refreshMetricOptions();
  });
  els.metric.addEventListener("change", refreshCaseOptions);
  els.case.addEventListener("change", render);
  els.range.addEventListener("input", render);
  let resizeTimeout;
  window.addEventListener("resize", () => {
    clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(() => {
      renderChart(visiblePoints());
    }, 100);
  });
}

main().catch((error) => {
  els.summary.textContent = `Failed to load benchmark history: ${error.message}`;
});
