import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import * as api from "@/api/client";

export const QK = {
  companies: ["companies"] as const,
  dashboard: (cik: string) => ["dashboard", cik] as const,
  events: (cik: string | null) => ["events", cik] as const,
  metricHistory: (cik: string, metric: string, kind: string) =>
    ["metricHistory", cik, metric, kind] as const,
  lineage: (id: number) => ["lineage", id] as const,
  currentValuation: (cik: string) => ["currentValuation", cik] as const,
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
