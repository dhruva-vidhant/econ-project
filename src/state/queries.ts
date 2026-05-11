import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import * as api from "@/api/client";

export const QK = {
  companies: ["companies"] as const,
  dashboard: (cik: string) => ["dashboard", cik] as const,
  events: (cik: string | null) => ["events", cik] as const,
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
