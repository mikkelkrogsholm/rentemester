/** Read-only ranking returned by the server; confidence is a score, not a probability. */
export type BankMatchKind = "issued_invoice" | "purchase_sale" | "credit_note_refund" | "supplier_credit_refund";
export type BankMatchSuggestion = {
  kind: BankMatchKind;
  documentId: number;
  invoiceNo: string | null;
  customerName?: string | null;
  supplierName?: string | null;
  confidence: number;
  reasons: string[];
};
export type BankMatchRow = {
  bankTransactionId: number;
  date: string;
  text: string;
  amount: number;
  currency: string;
  reference: string | null;
  suggestions: BankMatchSuggestion[];
  unsafeMatchReason: string | null;
};
export type BankMatchSuggestions = {ok: boolean; count: number; rows: BankMatchRow[]; errors: string[]};
