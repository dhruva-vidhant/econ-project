import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { CustomMetric } from "@/api/types";

import CustomMetricsPanel from "./CustomMetricsPanel";

vi.mock("@/api/client", () => ({
  listCustomMetrics: vi.fn(),
}));

import * as api from "@/api/client";

function renderWithClient(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const FIXTURE_METRIC: CustomMetric = {
  id: 1,
  cik: "0000000001",
  key: "tpv",
  label: "Total Payment Volume",
  unit_kind: "currency",
  currency: "USD",
  decimals: 2,
  description: null,
  created_at: "2024-01-01T00:00:00Z",
  updated_at: "2024-01-01T00:00:00Z",
};

describe("CustomMetricsPanel", () => {
  it("renders a persistent 'user data' badge and no lineage-drawer trigger", async () => {
    vi.mocked(api.listCustomMetrics).mockResolvedValue([FIXTURE_METRIC]);

    renderWithClient(<CustomMetricsPanel cik="0000000001" />);

    expect(await screen.findByText("user data")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("tpv")).toBeInTheDocument());

    // Trust separation (FR-CM-009): custom metrics never expose lineage.
    expect(screen.queryByLabelText("Lineage details")).toBeNull();
    expect(screen.queryByText(/lineage/i)).toBeNull();
  });

  it("shows an empty state when there are no custom metrics", async () => {
    vi.mocked(api.listCustomMetrics).mockResolvedValue([]);

    renderWithClient(<CustomMetricsPanel cik="0000000001" />);

    expect(await screen.findByText(/No custom metrics yet/i)).toBeInTheDocument();
    expect(screen.getByText("user data")).toBeInTheDocument();
  });
});
