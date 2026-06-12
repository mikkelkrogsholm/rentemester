// #343 — Retention status view.

export type RetentionStatusRow = {
  table: "documents" | "journal_entries" | "bank_transactions";
  total: number;
  expired: number;
  nextExpiry: string | null;
  oldestExpired: string | null;
};

export type CompanyRetention = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  report: {
    ok: boolean;
    asOf: string;
    appliedRules: string[];
    rows: RetentionStatusRow[];
    errors: string[];
  };
  legalCitation: {
    sourceId: string;
    note: string;
  };
};

export type RetentionResponse = {
  ok: true;
  retention: CompanyRetention;
};
