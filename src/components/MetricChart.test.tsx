import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { MetricSeriesPoint, Period } from "@/api/types";

import MetricChart from "./MetricChart";

let lastOption: Record<string, unknown> | undefined;
vi.mock("echarts-for-react", () => ({
  default: (props: { option: Record<string, unknown> }) => {
    lastOption = props.option;
    return <div data-testid="echarts-mock" />;
  },
}));

function period(fy: number): Period {
  return {
    id: fy,
    cik: "0000000001",
    fiscal_year: fy,
    fiscal_quarter: 0,
    fiscal_year_end: "1231",
    start_date: `${fy}-01-01`,
    end_date: `${fy}-12-31`,
    kind: "annual",
    is_53_week: false,
  };
}

function series(value: number): MetricSeriesPoint[] {
  return [{ period: period(2024), value, source_kind: "user_data", normalized_fact_id: -1 }];
}

function axisLabelFormatter(): (v: number) => string {
  const yAxis = lastOption?.yAxis as { axisLabel: { formatter: (v: number) => string } };
  return yAxis.axisLabel.formatter;
}

describe("MetricChart unit formatting", () => {
  it("formats a count series as a compact integer on the axis", () => {
    render(<MetricChart series={series(5_000_000)} unitKind="count" decimals={0} />);
    expect(axisLabelFormatter()(5)).not.toContain("%");
    expect(axisLabelFormatter()(5)).not.toContain("$");
  });

  it("formats a ratio series as a percentage on the axis", () => {
    render(<MetricChart series={series(42_000_000)} unitKind="ratio" />);
    expect(axisLabelFormatter()(42)).toBe("42.0%");
  });

  it("formats a currency series as compact USD (no leading $) on the axis", () => {
    render(<MetricChart series={series(1_500_000_000_000)} unitKind="currency" />);
    const label = axisLabelFormatter()(1_500_000);
    expect(label).not.toContain("$");
    expect(label).not.toContain("%");
  });
});
