import type {
  CompanyListResponse,
  CompanyProfileInput,
  CompanySettingsResponse,
  CreateCompanyInput,
  SyncCvrResponse,
  UpdateCompanyInput,
} from "../types";
import { request } from "./_shared";

export const companiesApi = {
  companies: () =>
    request<CompanyListResponse>("/api/companies").then((r) => r.companies),

  createCompany: (input: CreateCompanyInput) =>
    request<{ ok: true; company: { slug: string; name: string } }>(
      "/api/companies",
      { method: "POST", body: JSON.stringify(input) },
    ).then((r) => r.company),

  updateCompany: (slug: string, input: UpdateCompanyInput) =>
    request<{ ok: true; company: { slug: string; name: string; archived: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}`,
      { method: "PATCH", body: JSON.stringify(input) },
    ).then((r) => r.company),

  companySettings: (slug: string) =>
    request<CompanySettingsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/company`,
    ).then((r) => r.company),

  /**
   * Updates the editable company profile + bank/payment details (#284). Only
   * the fields present in `input` are changed. The primary bank account it
   * creates is the one every issued-invoice payment block reads from.
   */
  updateCompanyProfile: (slug: string, input: CompanyProfileInput) =>
    request<CompanySettingsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/company`,
      { method: "PATCH", body: JSON.stringify(input) },
    ).then((r) => r.company),

  syncCvr: (slug: string) =>
    request<SyncCvrResponse>(
      `/api/companies/${encodeURIComponent(slug)}/sync-cvr`,
      { method: "POST" },
    ).then((r) => r.sync),
};
