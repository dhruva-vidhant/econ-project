import ReactECharts from "echarts-for-react";

import { fmtCustomValue, fmtUsdCompact, isRatioMetric, microToPercent, microToUsd } from "@/api/types";
import type { MetricSeriesPoint, UnitKind } from "@/api/types";

interface Props {
  series: MetricSeriesPoint[];
  title?: string;
  height?: number;
  /** Metric id — drives unit-aware (USD vs percentage) axis/tooltip formatting. */
  metric?: string;
  /** Custom-metric unit kind (§9.2) — overrides `metric`-based ratio detection
   * when charting a custom series, which has no built-in `Metric` id. */
  unitKind?: UnitKind;
  /** Decimals for `unitKind="number"` formatting. */
  decimals?: number;
}

/** Time-series line chart for a single metric (M38). */
export default function MetricChart({ series, title, height = 320, metric, unitKind, decimals = 2 }: Props) {
  const ratio = unitKind ? unitKind === "ratio" : isRatioMetric(metric ?? "");
  if (series.length === 0) {
    return (
      <div
        className="flex items-center justify-center rounded border border-border/60 bg-surface text-sm text-muted"
        style={{ height }}
      >
        No data for this metric.
      </div>
    );
  }

  const xAxis = series.map((p) =>
    p.period.kind === "quarterly"
      ? `FY${p.period.fiscal_year} Q${p.period.fiscal_quarter}`
      : `FY${p.period.fiscal_year}`,
  );
  const data = series.map((p) => (ratio ? microToPercent(p.value) : microToUsd(p.value)));
  const formatValue = (raw: number) => {
    if (unitKind === "count" || unitKind === "number") {
      return fmtCustomValue(unitKind, raw * 1_000_000, decimals);
    }
    return ratio ? `${raw.toFixed(1)}%` : fmtUsdCompact(raw * 1_000_000);
  };

  const option = {
    backgroundColor: "transparent",
    title: title
      ? { text: title, left: "left", textStyle: { color: "#cfd6e3", fontSize: 13, fontWeight: 600 } }
      : undefined,
    tooltip: {
      trigger: "axis",
      formatter: (params: { axisValue: string; data: number }[]) => {
        const p = params[0];
        return `${p.axisValue}: ${formatValue(p.data)}`;
      },
      backgroundColor: "rgba(22,25,32,0.95)",
      borderColor: "#242933",
      textStyle: { color: "#e6e9f0", fontSize: 12 },
    },
    grid: { left: 60, right: 20, top: title ? 36 : 12, bottom: 30 },
    xAxis: {
      type: "category",
      data: xAxis,
      axisLine: { lineStyle: { color: "#3a4150" } },
      axisLabel: { color: "#8c95a8", fontSize: 11 },
    },
    yAxis: {
      type: "value",
      axisLine: { lineStyle: { color: "#3a4150" } },
      splitLine: { lineStyle: { color: "rgba(58, 65, 80, 0.4)" } },
      axisLabel: {
        color: "#8c95a8",
        fontSize: 11,
        formatter: (v: number) => formatValue(v).replace("$", ""),
      },
    },
    series: [
      {
        type: "line",
        data,
        smooth: false,
        symbol: "circle",
        symbolSize: 5,
        itemStyle: { color: "#569cd6" },
        lineStyle: { color: "#569cd6", width: 1.5 },
        areaStyle: { color: "rgba(86, 156, 214, 0.12)" },
        connectNulls: false,
      },
    ],
    animation: false,
  };

  return (
    <div className="rounded border border-border/60 bg-surface p-3" style={{ height: height + 24 }}>
      <ReactECharts option={option} style={{ height }} notMerge lazyUpdate />
    </div>
  );
}
