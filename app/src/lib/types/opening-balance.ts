export type OpeningInput = {
  cutOverDate: string;
  lines: Array<{ accountNo: string; debitAmount: string; creditAmount: string; text: string | null }>;
  note: string | null;
  sourceArchiveImportIds: number[];
};
export type OpeningSourceBasis = {
  kind: "operator_supplied" | "synthetic_csv";
  currency: string;
  sourceArchives: Array<{ importId: number; sourceHash: string; parserContract: string; declaredCurrency: string }>;
};
export type OpeningRecord = {
  cutOverDate: string; journalEntryId: number; journalEntryNo: string; journalEntryHash: string;
  planHash: string; sourceBasis: OpeningSourceBasis; createdAt: string; actor: string;
};
export type OpeningPlan = {
  planHash: string; input: OpeningInput; ruleVersion: string; sourceBasis: OpeningSourceBasis;
  totals: { debit: string; credit: string }; existing: OpeningRecord | null;
  blockers: string[]; status: "ready" | "blocked";
};
export type OpeningApply = { input: OpeningInput; planHash: string; idempotencyKey: string; confirm: true };
export type OpeningState = { ok: true; openingBalance: OpeningRecord | null; errors: string[] };
export type OpeningReceipt = OpeningState & {
  idempotency: { receiptId: number; replayed: boolean; createdAt: string; expiresAt: string };
};
