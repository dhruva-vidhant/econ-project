import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import * as api from "@/api/client";
import type { CustomMetricPatch, ImportOptions, NewCustomMetric } from "@/api/types";

export const QK = {
  companies: ["companies"] as const,
  dashboard: (cik: string) => ["dashboard", cik] as const,
  events: (cik: string | null) => ["events", cik] as const,
  metricHistory: (cik: string, metric: string, kind: string) =>
    ["metricHistory", cik, metric, kind] as const,
  lineage: (id: number) => ["lineage", id] as const,
  currentValuation: (cik: string) => ["currentValuation", cik] as const,
  customMetrics: (cik: string) => ["custom-metrics", cik] as const,
  customSeries: (cik: string, metricId: number, kind: string) =>
    ["custom-series", cik, metricId, kind] as const,
};

export function useCompanies() {
  return useQuery({
    queryKey: QK.companies,
    queryFn: api.listCompanies,
  });
}

export function useDashboard(cik: string | undefined) {
  return useQuery({
    queryKey: cik ? QK.dashboard(cik) : ["dashboard", "none"],
    queryFn: () => api.getDashboard(cik!),
    enabled: !!cik,
  });
}

export function useEvents(cik: string | null = null, limit = 200) {
  return useQuery({
    queryKey: QK.events(cik),
    queryFn: () => api.getIngestionEvents(cik, limit),
  });
}

export function useAddCompany() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.addCompany,
    onSuccess: () => qc.invalidateQueries({ queryKey: QK.companies }),
  });
}

export function useRemoveCompany() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ cik, dropCache }: { cik: string; dropCache: boolean }) =>
      api.removeCompany(cik, dropCache),
    onSuccess: () => qc.invalidateQueries({ queryKey: QK.companies }),
  });
}

export function useMetricHistory(
  cik: string | undefined,
  metric: string,
  kind: "annual" | "quarterly",
) {
  return useQuery({
    queryKey: cik ? QK.metricHistory(cik, metric, kind) : ["metricHistory", "none"],
    queryFn: () => api.getMetricHistory(cik!, metric, kind),
    enabled: !!cik,
  });
}

export function useLineage(normalizedFactId: number | undefined) {
  return useQuery({
    queryKey: normalizedFactId ? QK.lineage(normalizedFactId) : ["lineage", "none"],
    queryFn: () => api.getLineage(normalizedFactId!),
    enabled: !!normalizedFactId,
  });
}

export function useRefreshCompany() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.refreshCompany,
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: QK.companies });
      qc.invalidateQueries({ queryKey: QK.dashboard(data.company.cik) });
      qc.invalidateQueries({ queryKey: QK.events(data.company.cik) });
      qc.invalidateQueries({ queryKey: ["metricHistory", data.company.cik] });
      qc.invalidateQueries({ queryKey: QK.currentValuation(data.company.cik) });
      qc.invalidateQueries({ queryKey: QK.customMetrics(data.company.cik) });
      // Re-resolution (D2) may realign previously-unaligned custom points.
      qc.invalidateQueries({ queryKey: ["custom-series", data.company.cik] });
    },
  });
}

export function useCustomMetrics(cik: string | undefined) {
  return useQuery({
    queryKey: cik ? QK.customMetrics(cik) : ["custom-metrics", "none"],
    queryFn: () => api.listCustomMetrics(cik!),
    enabled: !!cik,
  });
}

export function useCustomMetricSeries(
  cik: string | undefined,
  metricId: number | undefined,
  kind: "annual" | "quarterly",
) {
  return useQuery({
    queryKey: cik && metricId ? QK.customSeries(cik, metricId, kind) : ["custom-series", "none"],
    queryFn: () => api.getCustomMetricSeries(cik!, metricId!, kind),
    enabled: !!cik && !!metricId,
  });
}

export function useCreateCustomMetric(cik: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (def: NewCustomMetric) => api.createCustomMetric(cik, def),
    onSuccess: () => qc.invalidateQueries({ queryKey: QK.customMetrics(cik) }),
  });
}

export function useUpdateCustomMetric(cik: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, patch }: { id: number; patch: CustomMetricPatch }) =>
      api.updateCustomMetric(id, patch),
    onSuccess: () => qc.invalidateQueries({ queryKey: QK.customMetrics(cik) }),
  });
}

export function useDeleteCustomMetric(cik: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.deleteCustomMetric(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: QK.customMetrics(cik) });
      qc.invalidateQueries({ queryKey: ["custom-series", cik] });
    },
  });
}

export function useUpsertCustomMetricPoint(cik: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      metricId,
      period,
      kind,
      value,
    }: {
      metricId: number;
      period: string;
      kind: "annual" | "quarterly";
      value: string;
    }) => api.upsertCustomMetricPoint(metricId, period, kind, value),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["custom-series", cik] }),
  });
}

export function useImportCustomMetricsCsv(cik: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ csvText, options }: { csvText: string; options: ImportOptions }) =>
      api.importCustomMetricsCsv(cik, csvText, options),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: QK.customMetrics(cik) });
      qc.invalidateQueries({ queryKey: ["custom-series", cik] });
    },
  });
}

export function useCurrentValuation(cik: string | undefined) {
  return useQuery({
    queryKey: cik ? QK.currentValuation(cik) : ["currentValuation", "none"],
    queryFn: () => api.getCurrentValuation(cik!),
    enabled: !!cik,
  });
}

export function useRefreshPrice() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.refreshPrice,
    onSuccess: (data, cik) => {
      qc.setQueryData(QK.currentValuation(cik), data);
      qc.invalidateQueries({ queryKey: ["metricHistory", cik, "free_cash_flow_yield", "annual"] });
    },
  });
}
