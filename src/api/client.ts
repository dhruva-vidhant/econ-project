import { invoke } from "@tauri-apps/api/core";
import type {
  AddCompanyResponse,
  Company,
  CurrentValuation,
  DashboardPayload,
  IngestionEvent,
  LineagePayload,
  MetricSeriesPoint,
} from "@/api/types";

/**
 * Typed Tauri IPC client. One function per command in M29.
 * The bindings here are kept in sync with `src-tauri/src/ipc/commands.rs`.
 */

export async function ping(): Promise<{ message: string; version: string }> {
  return invoke("ping");
}

export async function listCompanies(): Promise<Company[]> {
  return invoke("list_companies");
}

export async function addCompany(ticker: string): Promise<AddCompanyResponse> {
  return invoke("add_company", { ticker });
}

export async function removeCompany(cik: string, dropCache: boolean): Promise<void> {
  return invoke("remove_company", { cik, dropCache });
}

export async function getDashboard(cik: string): Promise<DashboardPayload> {
  return invoke("get_dashboard", { cik });
}

export async function getIngestionEvents(
  cik: string | null,
  limit: number,
): Promise<IngestionEvent[]> {
  return invoke("get_ingestion_events", { cik, limit });
}

export async function getMetricHistory(
  cik: string,
  metric: string,
  kind: "annual" | "quarterly",
): Promise<MetricSeriesPoint[]> {
  return invoke("get_metric_history", { cik, metric, kind });
}

export async function getLineage(normalizedFactId: number): Promise<LineagePayload> {
  return invoke("get_lineage", { normalizedFactId });
}

export async function refreshCompany(cik: string): Promise<AddCompanyResponse> {
  return invoke("refresh_company", { cik });
}

export async function getCurrentValuation(cik: string): Promise<CurrentValuation | null> {
  return invoke("get_current_valuation", { cik });
}

export async function refreshPrice(cik: string): Promise<CurrentValuation | null> {
  return invoke("refresh_price", { cik });
}
