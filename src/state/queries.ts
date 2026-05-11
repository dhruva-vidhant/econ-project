import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import * as api from "@/api/client";

export const QK = {
  companies: ["companies"] as const,
};

export function useCompanies() {
  return useQuery({
    queryKey: QK.companies,
    queryFn: api.listCompanies,
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
