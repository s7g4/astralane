const mintSelect = document.getElementById("mint-select");
const intervalSelect = document.getElementById("interval-select");
const chartContainer = document.getElementById("chart");
const ohlcvEmpty = document.getElementById("ohlcv-empty");

const chart = LightweightCharts.createChart(chartContainer, {
  layout: { background: { color: "#171a21" }, textColor: "#e6e6e6" },
  grid: {
    vertLines: { color: "#2a2e38" },
    horzLines: { color: "#2a2e38" },
  },
  timeScale: { timeVisible: true },
});
const candleSeries = chart.addCandlestickSeries();

window.addEventListener("resize", () => {
  chart.applyOptions({ width: chartContainer.clientWidth });
});

async function loadTokens() {
  const res = await fetch("/api/tokens");
  const tokens = await res.json();
  mintSelect.innerHTML = "";
  for (const t of tokens) {
    const opt = document.createElement("option");
    opt.value = t.mint;
    const marker = t.has_candles ? "" : " — no trades matched";
    opt.textContent = `${t.mint.slice(0, 8)}... (${t.activity_count})${marker}`;
    mintSelect.appendChild(opt);
  }
  if (tokens.length > 0) {
    await loadOhlcv();
  }
}

async function loadOhlcv() {
  const mint = mintSelect.value;
  const interval = intervalSelect.value;
  if (!mint) return;

  const res = await fetch(`/api/ohlcv?mint=${encodeURIComponent(mint)}&interval=${interval}`);
  const data = await res.json();

  if (data.candles.length === 0) {
    candleSeries.setData([]);
    ohlcvEmpty.classList.remove("hidden");
    return;
  }
  ohlcvEmpty.classList.add("hidden");

  const bars = data.candles.map((c) => ({
    time: c.bucket_start,
    open: c.open,
    high: c.high,
    low: c.low,
    close: c.close,
  }));
  candleSeries.setData(bars);
  chart.timeScale().fitContent();
}

async function loadContention() {
  const res = await fetch("/api/contention");
  const data = await res.json();

  const summary = document.getElementById("contention-summary");
  if (data.blocks.length === 0) {
    summary.innerHTML = `<div>Contention summary not computed yet.</div>`;
    return;
  }

  const depths = data.blocks.map((b) => b.schedule_depth);
  const widths = data.blocks.flatMap((b) => b.step_widths);
  // Spreading into Math.max/min (Math.max(...arr)) blows the call stack once
  // arr has more than ~65k-125k elements - widths here can have 200k+
  // (sum of every step's width across 1000 blocks), so reduce instead.
  const avg = (arr) => (arr.length ? arr.reduce((a, b) => a + b, 0) / arr.length : 0);
  const min = (arr) => arr.reduce((a, b) => (b < a ? b : a), arr[0]);
  const max = (arr) => arr.reduce((a, b) => (b > a ? b : a), arr[0]);

  summary.innerHTML = `
    <div><strong>${data.block_count}</strong>blocks analyzed</div>
    <div><strong>${min(depths)} / ${max(depths)} / ${avg(depths).toFixed(1)}</strong>schedule depth (min/max/avg)</div>
    <div><strong>${max(widths)} / ${avg(widths).toFixed(1)}</strong>step width (max/avg)</div>
  `;

  const fillTable = (tbodyId, rows) => {
    const tbody = document.querySelector(`#${tbodyId} tbody`);
    tbody.innerHTML = rows
      .map(
        (r) =>
          `<tr><td>${r.key}</td><td>${r.write_conflicts.toLocaleString()}</td><td>${r.read_conflicts.toLocaleString()}</td></tr>`
      )
      .join("");
  };
  fillTable("accounts-table", data.top_accounts);
  fillTable("programs-table", data.top_programs);
}

mintSelect.addEventListener("change", loadOhlcv);
intervalSelect.addEventListener("change", loadOhlcv);

loadTokens();
loadContention();
