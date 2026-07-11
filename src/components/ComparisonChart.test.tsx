import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { Period } from "@/api/types";

import ComparisonChart from "./ComparisonChart";
import type { NamedSeries } from "./ComparisonChart";

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

describe("ComparisonChart", () => {
  it("groups currency and count series onto different y-axes", () => {
    const series: NamedSeries[] = [
      { label: "revenue", unitKind: "currency", points: [{ period: period(2024), value_micro: 1_000_000_000_000 }] },
      { label: "active_accounts", unitKind: "count", points: [{ period: period(2024), value_micro: 500_000_000 }] },
    ];

    render(<ComparisonChart series={series} />);

    const chartSeries = lastOption?.series as { name: string; yAxisIndex: number }[];
    const revenueSeries = chartSeries.find((s) => s.name === "revenue")!;
    const accountsSeries = chartSeries.find((s) => s.name === "active_accounts")!;
    expect(revenueSeries.yAxisIndex).not.toBe(accountsSeries.yAxisIndex);

    const yAxis = lastOption?.yAxis as { position: string }[];
    expect(yAxis).toHaveLength(2);
    expect(yAxis[0].position).toBe("left");
    expect(yAxis[1].position).toBe("right");
  });

  it("shows an empty state when no series has data", () => {
    const { getByText } = render(<ComparisonChart series={[]} />);
    expect(getByText("No data to compare.")).toBeInTheDocument();
  });
});
