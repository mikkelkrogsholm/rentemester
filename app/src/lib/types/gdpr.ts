// #334 — GDPR-export og forget UI.

export type GdprPersonalData = {
  name: string | null;
  address: string | null;
  email: string | null;
  vatOrCvr: string | null;
};

export type GdprExportRecord = {
  source:
    | "customers"
    | "vendors"
    | "documents"
    | "bank_transactions"
    | "journal_entries"
    | "journal_lines"
    | "audit_log";
  sourceRowId: number;
  label: string | null;
  personalData: GdprPersonalData;
  retainUntil: string | null;
  underRetention: boolean;
  retentionUnknown?: boolean;
  erased: boolean;
  erasable: boolean;
};

export type GdprSubjectExport = {
  coverage?: { complete: boolean; notice: string };
  ok: boolean;
  asOf: string;
  appliedRules: string[];
  subject: { cvr: string | null; name: string | null };
  records: GdprExportRecord[];
  errors: string[];
};

export type CompanyGdpr = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  export: GdprSubjectExport;
};

export type GdprResponse = {
  ok: true;
  gdpr: CompanyGdpr;
};

export type GdprErasureResult = {
  ok: boolean;
  asOf: string;
  subject: { cvr: string | null; name: string | null };
  erasedCount: number;
  refusedCount: number;
  alreadyErasedCount: number;
  erased: Array<{
    source: string;
    sourceRowId: number;
    label: string | null;
    redactedFields: string[];
  }>;
  refused: Array<{
    source: string;
    sourceRowId: number;
    label: string | null;
    retainUntil: string | null;
    reason: string;
  }>;
  errors: string[];
};
