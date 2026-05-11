import { invoke } from "@tauri-apps/api/core";
import type { Company } from "@/api/types";

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

export async function addCompany(ticker: string): Promise<Company> {
  return invoke("add_company", { ticker });
}

export async function removeCompany(cik: string, dropCache: boolean): Promise<void> {
  return invoke("remove_company", { cik, dropCache });
}
