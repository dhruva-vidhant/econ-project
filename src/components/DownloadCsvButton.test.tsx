import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ExportRequest } from "@/api/types";

import DownloadCsvButton from "./DownloadCsvButton";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(),
}));
vi.mock("@/api/client", () => ({
  exportCsv: vi.fn(),
}));

import { save } from "@tauri-apps/plugin-dialog";
import * as api from "@/api/client";

const REQUEST: ExportRequest = { type: "metric_series", cik: "0000000001", metric: "revenue", kind: "annual" };

describe("DownloadCsvButton", () => {
  afterEach(() => vi.clearAllMocks());

  it("invokes export_csv with the exact request and the chosen path", async () => {
    vi.mocked(save).mockResolvedValue("/tmp/AAPL_revenue_annual.csv");
    vi.mocked(api.exportCsv).mockResolvedValue({ rows: 5, bytes: 200, path: "/tmp/AAPL_revenue_annual.csv" });

    render(<DownloadCsvButton request={REQUEST} defaultFilename="AAPL_revenue_annual.csv" />);
    fireEvent.click(screen.getByRole("button", { name: "Download CSV" }));

    await waitFor(() => expect(api.exportCsv).toHaveBeenCalledTimes(1));
    expect(api.exportCsv).toHaveBeenCalledWith(REQUEST, "/tmp/AAPL_revenue_annual.csv");
  });

  it("does not export when the user cancels the save dialog", async () => {
    vi.mocked(save).mockResolvedValue(null);

    render(<DownloadCsvButton request={REQUEST} defaultFilename="AAPL_revenue_annual.csv" />);
    fireEvent.click(screen.getByRole("button", { name: "Download CSV" }));

    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    expect(api.exportCsv).not.toHaveBeenCalled();
  });
});
