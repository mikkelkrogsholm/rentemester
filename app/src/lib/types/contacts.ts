// Contacts / Kontakter wire types — customer + vendor rows, edit payloads,
// CVR-lookup result (#390).

import type { FiscalYearEntry, StatementCompany } from "./common";

export type ContactCustomerRow = {
  id: number;
  name: string;
  vatOrCvr: string | null;
  email: string | null;
  paymentTermsDays: number;
  defaultCurrency: string;
  // #390 — full stamdata so the edit-modal can prefill without another fetch.
  address: string | null;
  phone: string | null;
  website: string | null;
  eanNumber: string | null;
  notes: string | null;
  /**
   * #439 — aggregated åbent tilgodehavende på tværs af alle år, kroner.
   * Server-side derivat fra samme ledger-kilde som `/invoices`-endpointet.
   * `0` når kunden ingen åbne fakturaer har.
   */
  openBalance: number;
  /** #439 — antal åbne (endnu ikke fuldt betalte) fakturaer for kunden. */
  openInvoiceCount: number;
  /**
   * #439 — antal af kundens åbne fakturaer der er løbet over forfaldsdato.
   * `> 0` udløser den røde flag-styling i Kontakter-tabellen.
   */
  overdueCount: number;
};

export type ContactVendorRow = {
  id: number;
  name: string;
  vatOrCvr: string | null;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
  // #390 — full stamdata so the edit-modal can prefill without another fetch.
  address: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  notes: string | null;
};

// --- contact create/update payloads (#390) ----------------------------------

export type CustomerInput = {
  name: string;
  address?: string | null;
  vatOrCvr?: string | null;
  email?: string | null;
  phone?: string | null;
  website?: string | null;
  eanNumber?: string | null;
  paymentTermsDays?: number;
  defaultCurrency?: string;
  notes?: string | null;
};

export type VendorInput = {
  name: string;
  address?: string | null;
  vatOrCvr?: string | null;
  email?: string | null;
  phone?: string | null;
  website?: string | null;
  defaultExpenseAccount?: string | null;
  defaultVatTreatment?: string | null;
  notes?: string | null;
};

/** CVR-lookup result the cockpit modal uses to prefill name + address. */
export type CvrLookupResult = {
  ok: boolean;
  cached: boolean;
  company: {
    cvr: string;
    name: string;
    address?: string | null;
    postalCode?: string | null;
    city?: string | null;
    email?: string | null;
    phone?: string | null;
    website?: string | null;
  } | null;
  errors: string[];
};

export type CompanyContacts = {
  slug: string;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  customers: ContactCustomerRow[];
  vendors: ContactVendorRow[];
};

export type ContactsResponse = {
  ok: true;
  contacts: CompanyContacts;
};
