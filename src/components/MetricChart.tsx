import ReactECharts from "echarts-for-react";

import { fmtUsdCompact, microToUsd } from "@/api/types";
import type { MetricSeriesPoint } from "@/api/types";

interface Props {
  series: MetricSeriesPoint[];
  title?: string;
  height?: number;
}

/** Time-series line chart for a single metric (M38). */
export default function MetricChart({ series, title, height = 320 }: Props) {
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
  const data = series.map((p) => microToUsd(p.value));

  const option = {
    backgroundColor: "transparent",
    title: title
      ? { text: title, left: "left", textStyle: { color: "#cfd6e3", fontSize: 13, fontWeight: 600 } }
      : undefined,
    tooltip: {
      trigger: "axis",
      formatter: (params: { axisValue: string; data: number }[]) => {
        const p = params[0];
        return `${p.axisValue}: ${fmtUsdCompact(p.data * 1_000_000)}`;
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
        formatter: (v: number) => fmtUsdCompact(v * 1_000_000).replace("$", ""),
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
