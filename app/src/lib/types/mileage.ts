// Mileage / Kørsel wire types (GET .../mileage?year=) — #335.

import type { FiscalYearEntry, StatementCompany } from "./common";

export type MileageEntryRow = {
  id: number;
  /** Sequence number `MIL-{year}-{6 digits}` assigned by the core. */
  entryNo: string;
  tripDate: string;
  purpose: string;
  fromLocation: string;
  toLocation: string;
  /** Kilometres driven, rounded to hundredths. */
  kilometers: number;
  vehicle: string;
  driver: string;
  /** User-supplied per-kilometre rate (kr). The mileage core never owns a tax rate. */
  ratePerKm: number;
  /** `kilometers * ratePerKm`, rounded to øre. Documentation only — never a posted amount. */
  amountBasis: number;
  /** Free-text, source-backed basis the user confirms (which official rate table). */
  rateBasis: string;
  rateSource: string | null;
  notes: string | null;
  createdAt: string;
};

export type MileageMonthRow = {
  /** 1–12. */
  month: number;
  /** Danish month abbreviation (`jan`, `feb`, …). */
  label: string;
  tripCount: number;
  kilometers: number;
  amountBasis: number;
};

export type CompanyMileage = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  /** Newest trip first. */
  entries: MileageEntryRow[];
  totalKilometers: number;
  totalAmountBasis: number;
  tripCount: number;
  /** Twelve rows in fiscal-year order; months with no trips appear with zero values. */
  months: MileageMonthRow[];
};

export type MileageResponse = {
  ok: true;
  mileage: CompanyMileage;
};

/** Input for `api.createMileageEntry` — mirrors `CreateMileageEntryInput` in the core. */
export type MileageEntryInput = {
  tripDate: string;
  purpose: string;
  fromLocation: string;
  toLocation: string;
  kilometers: number;
  vehicle: string;
  driver: string;
  ratePerKm: number;
  rateBasis: string;
  rateSource?: string;
  notes?: string;
};

/** The create result the server echoes back. */
export type MileageEntrySummary = {
  mileageEntryId: number | null;
  entryNo: string | null;
  amountBasis: number | null;
  warnings?: string[];
};

/** Additive native accounting contract; decimal rates remain exact strings. */
export type MileageRateInput = {
  version: string;
  validFrom: string;
  validTo: string;
  vehicle: string;
  ratePerKm: string;
  basis: string;
  source: string;
  expenseAccountNo: string;
  liabilityAccountNo: string;
};
export type MileageRateVersion = MileageRateInput & { actor: string; createdAt: string };
export type MileagePostPreviewInput = {
  mileageEntryId: number;
  rateVersion: string;
  documentId: number;
};
export type MileagePostPreview = MileagePostPreviewInput & {
  entryNo: string;
  tripDate: string;
  vehicle: string;
  kilometers: number;
  ratePerKm: string;
  amount: number;
  expenseAccountNo: string;
  liabilityAccountNo: string;
  documentHash: string;
  previewHash: string;
  appliedRules: ["NATIVE-MILEAGE-POST-1"];
};
export type MileagePostInput = MileagePostPreviewInput & {
  expectedPreviewHash: string;
  operationKey: string;
  confirm: true;
};
export type MileagePostResult = MileagePostPreview & {
  journalEntryId: number;
  journalEntryNo: string;
  replayed: boolean;
};
