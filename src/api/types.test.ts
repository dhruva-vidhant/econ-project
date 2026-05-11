import { describe, expect, it } from "vitest";

import { fmtUsdCompact, microToUsd } from "@/api/types";

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
