import { describe, expect, it } from "vitest";

import {
  fmtMetricValue,
  fmtPercent,
  fmtUsdCompact,
  isRatioMetric,
  microToPercent,
  microToUsd,
} from "@/api/types";

describe("fmtUsdCompact", () => {
  it("formats trillions", () => {
    expect(fmtUsdCompact(2_500_000_000_000 * 1_000_000)).toBe("$2.50T");
  });
  it("formats billions", () => {
    expect(fmtUsdCompact(383_285_000_000 * 1_000_000)).toBe("$383B");
  });
  it("formats millions", () => {
    expect(fmtUsdCompact(50_000_000 * 1_000_000)).toBe("$50.00M");
  });
  it("preserves negative sign", () => {
    expect(fmtUsdCompact(-1_500_000 * 1_000_000)).toBe("-$1.50M");
  });
});

describe("microToUsd", () => {
  it("scales down by 1e6", () => {
    expect(microToUsd(1_234_567)).toBeCloseTo(1.234567);
  });
});

describe("ratio metrics", () => {
  it("classifies operating_margin as a ratio, monetary metrics as not", () => {
    expect(isRatioMetric("operating_margin")).toBe(true);
    expect(isRatioMetric("free_cash_flow")).toBe(false);
    expect(isRatioMetric("revenue")).toBe(false);
  });

  it("microToPercent converts ratio micro-units to a percentage number", () => {
    // 354_917 micro-ratio (Zoetis FY2025 operating margin) → 35.49%
    expect(microToPercent(354_917)).toBeCloseTo(35.4917);
  });

  it("fmtPercent renders one-decimal percentages", () => {
    expect(fmtPercent(354_917)).toBe("35.5%");
    expect(fmtPercent(45_982)).toBe("4.6%");
    expect(fmtPercent(-100_000)).toBe("-10.0%");
  });

  it("fmtMetricValue dispatches on metric unit", () => {
    // ratio → percent, everything else → compact USD
    expect(fmtMetricValue("operating_margin", 200_000)).toBe("20.0%");
    expect(fmtMetricValue("free_cash_flow", 2_539_000_000 * 1_000_000)).toBe("$2.54B");
  });
});
