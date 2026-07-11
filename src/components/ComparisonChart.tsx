import ReactECharts from "echarts-for-react";

import { fmtCustomValue, fmtUsdCompact, microToPercent, microToUsd } from "@/api/types";
import type { Period, UnitKind } from "@/api/types";

export interface NamedSeries {
  label: string;
  unitKind: UnitKind;
  decimals?: number;
  points: { period: Period; value_micro: number }[];
}

interface Props {
  series: NamedSeries[];
  height?: number;
}

const COLORS = ["#569cd6", "#4ec9b0", "#dcb67a", "#c586c0", "#d16969"];

/** Overlays multiple series (built-in and/or custom) on one chart, grouping
 * axes by unit kind so currency and count/ratio series don't share a scale
 * (§9.2). Periods across series are aligned by their fiscal label. */
export default function ComparisonChart({ series, height = 360 }: Props) {
  if (series.length === 0 || series.every((s) => s.points.length === 0)) {
    return (
      <div
        className="flex items-center justify-center rounded border border-border/60 bg-surface text-sm text-muted"
        style={{ height }}
      >
        No data to compare.
      </div>
    );
  }

  const labelFor = (p: Period) =>
    p.kind === "quarterly" ? `FY${p.fiscal_year} Q${p.fiscal_quarter}` : `FY${p.fiscal_year}`;

  const allLabels = Array.from(
    new Set(series.flatMap((s) => s.points.map((p) => labelFor(p.period)))),
  ).sort();

  // Group series onto axes by unitKind: currency → left, others → right,
  // splitting further only if more than two distinct kinds appear.
  const kinds = Array.from(new Set(series.map((s) => s.unitKind)));
  const axisIndexForKind = new Map(kinds.map((k, i) => [k, Math.min(i, 1)]));

  const scale = (unitKind: UnitKind, raw: number) =>
    unitKind === "ratio" ? microToPercent(raw) : microToUsd(raw);

  const formatValue = (unitKind: UnitKind, decimals: number, raw: number) => {
    if (unitKind === "ratio") return `${raw.toFixed(1)}%`;
    if (unitKind === "count" || unitKind === "number") {
      return fmtCustomValue(unitKind, raw * 1_000_000, decimals);
    }
    return fmtUsdCompact(raw * 1_000_000);
  };

  const option = {
    backgroundColor: "transparent",
    tooltip: {
      trigger: "axis",
      backgroundColor: "rgba(22,25,32,0.95)",
      borderColor: "#242933",
      textStyle: { color: "#e6e9f0", fontSize: 12 },
    },
    legend: { textStyle: { color: "#8c95a8", fontSize: 11 }, top: 0 },
    grid: { left: 60, right: 60, top: 32, bottom: 30 },
    xAxis: {
      type: "category",
      data: allLabels,
      axisLine: { lineStyle: { color: "#3a4150" } },
      axisLabel: { color: "#8c95a8", fontSize: 11 },
    },
    yAxis: [0, 1].map((axisIndex) => ({
      type: "value",
      position: axisIndex === 0 ? "left" : "right",
      axisLine: { lineStyle: { color: "#3a4150" } },
      splitLine: { lineStyle: { color: "rgba(58, 65, 80, 0.4)" } },
      axisLabel: { color: "#8c95a8", fontSize: 11 },
    })),
    series: series.map((s, i) => {
      const byLabel = new Map(s.points.map((p) => [labelFor(p.period), scale(s.unitKind, p.value_micro)]));
      return {
        name: s.label,
        type: "line",
        yAxisIndex: axisIndexForKind.get(s.unitKind) ?? 0,
        data: allLabels.map((l) => byLabel.get(l) ?? null),
        symbol: "circle",
        symbolSize: 5,
        itemStyle: { color: COLORS[i % COLORS.length] },
        lineStyle: { color: COLORS[i % COLORS.length], width: 1.5 },
        connectNulls: false,
        tooltip: {
          valueFormatter: (v: number | string) =>
            typeof v === "number" ? formatValue(s.unitKind, s.decimals ?? 2, v) : "—",
        },
      };
    }),
    animation: false,
  };

  return (
    <div className="rounded border border-border/60 bg-surface p-3" style={{ height: height + 24 }}>
      <ReactECharts option={option} style={{ height }} notMerge lazyUpdate />
    </div>
  );
}
