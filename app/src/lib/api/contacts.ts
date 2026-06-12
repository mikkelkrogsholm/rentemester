import type {
  ContactsResponse,
  CustomerInput,
  CvrLookupResult,
  VendorInput,
} from "../types";
import { request } from "./_shared";

export const contactsApi = {
  contacts: (slug: string) =>
    request<ContactsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/contacts`,
    ).then((r) => r.contacts),

  /**
   * Creates a new customer (#390). Mirrors the CLI's `customer add`. The
   * Cockpit's Kontakter page calls this directly so the owner never has to
   * leave the browser for daily master-data maintenance.
   */
  createCustomer: (slug: string, input: CustomerInput) =>
    request<{ ok: true; customer: { id: number } }>(
      `/api/companies/${encodeURIComponent(slug)}/customers`,
      { method: "POST", body: JSON.stringify(input) },
    ).then((r) => r.customer),

  /** Updates an existing customer — only the fields present in `input` change. */
  updateCustomer: (slug: string, id: number, input: Partial<CustomerInput>) =>
    request<{ ok: true; customer: { id: number; ok: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/customers/${id}`,
      { method: "PATCH", body: JSON.stringify(input) },
    ).then((r) => r.customer),

  /**
   * #430 — sletter en kunde fra master data. Sletningen blokeres server-side
   * (returnerer ApiError) hvis kunden er i brug på en åben (ikke-betalt)
   * udstedt faktura; cockpittet viser fejlbeskeden verbatim. Bogførte
   * fakturaer beholder deres navne-snapshot — historikken er intakt.
   * Write-irreversibel, så body'en bærer `confirm: true`.
   */
  deleteCustomer: (slug: string, id: number) =>
    request<{ ok: true; customer: { id: number; deleted: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/customers/${id}`,
      { method: "DELETE", body: JSON.stringify({ confirm: true }) },
    ).then((r) => r.customer),

  /** Creates a new vendor (#390). */
  createVendor: (slug: string, input: VendorInput) =>
    request<{ ok: true; vendor: { id: number } }>(
      `/api/companies/${encodeURIComponent(slug)}/vendors`,
      { method: "POST", body: JSON.stringify(input) },
    ).then((r) => r.vendor),

  /** Updates an existing vendor — only the fields present in `input` change. */
  updateVendor: (slug: string, id: number, input: Partial<VendorInput>) =>
    request<{ ok: true; vendor: { id: number; ok: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/vendors/${id}`,
      { method: "PATCH", body: JSON.stringify(input) },
    ).then((r) => r.vendor),

  /**
   * #430 — sletter en leverandør. Blokeres hvis leverandøren er i brug på en
   * åben gæld (`payables` med `vendor_id`-FK). Write-irreversibel.
   */
  deleteVendor: (slug: string, id: number) =>
    request<{ ok: true; vendor: { id: number; deleted: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/vendors/${id}`,
      { method: "DELETE", body: JSON.stringify({ confirm: true }) },
    ).then((r) => r.vendor),

  /**
   * Looks an 8-digit Danish CVR number up in the CVR register (#390). The
   * credentials live on the server; the browser only ever sees the resolved
   * snapshot. A missing-credentials response is returned with `ok:false`
   * inside the envelope so the modal can show a calm hint, not an error.
   */
  cvrLookup: (slug: string, cvr: string) =>
    request<{ ok: true; cvr: CvrLookupResult }>(
      `/api/companies/${encodeURIComponent(slug)}/cvr-lookup?cvr=${encodeURIComponent(cvr)}`,
    ).then((r) => r.cvr),
};
