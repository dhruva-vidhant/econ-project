import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ImportReport } from "@/api/types";

import ImportCsvDialog from "./ImportCsvDialog";

vi.mock("@/api/client", () => ({
  importCustomMetricsCsv: vi.fn(),
}));

import * as api from "@/api/client";

function renderWithClient(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const FIXTURE_REPORT: ImportReport = {
  metrics_created: ["tpv", "payment_transactions"],
  imported: 14,
  updated: 2,
  unaligned: 1,
  rejected: [{ row_index: 3, reason: "bad_value", raw: "x" }],
};

describe("ImportCsvDialog", () => {
  it("reports the import summary after a successful import", async () => {
    vi.mocked(api.importCustomMetricsCsv).mockResolvedValue(FIXTURE_REPORT);

    renderWithClient(<ImportCsvDialog cik="0000000001" onClose={vi.fn()} />);

    const file = new File(["metric,period,value,unit\ntpv,FY2024,100,currency\n"], "kpis.csv", {
      type: "text/csv",
    });
    const input = screen.getByLabelText(/CSV file/i);
    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() => expect(screen.getByRole("button", { name: "Import" })).not.toBeDisabled());
    fireEvent.click(screen.getByRole("button", { name: "Import" }));

    expect(await screen.findByText(/Imported: 14/)).toBeInTheDocument();
    expect(screen.getByText(/Updated: 2/)).toBeInTheDocument();
    expect(screen.getByText(/Unaligned \(no matching period yet\): 1/)).toBeInTheDocument();
    expect(screen.getByText(/Rejected: 1/)).toBeInTheDocument();
    expect(screen.getByText(/tpv, payment_transactions/)).toBeInTheDocument();
    expect(screen.getByText(/row 3: bad_value/)).toBeInTheDocument();
  });
});
