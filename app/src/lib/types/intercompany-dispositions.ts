/** Exact evidence payload from the workspace disposition core. */
export type IntercompanyDisposition = {
  dispositionId: string;
  type: "invoice" | "reimbursement" | "loan" | "capital" | "dividend" | "repayment" | "other";
  economicDate: string; amount: number; currency: string; settlementDueDate?: string;
  partyIds: string[]; evidenceRecordIds: string[];
  left: { companySlug: string; role: string; expectedSide: "receivable" | "payable" };
  right: { companySlug: string; role: string; expectedSide: "receivable" | "payable" };
};
export type IntercompanyDispositionPlan = {
  ok: true; mode: "dry-run"; disposition: IntercompanyDisposition; payloadHash: string;
  ledgerEffects: never[]; proposedSides: string[]; warnings: string[];
};

export type IntercompanyDispositionAction = "plan" | "propose" | "approve" | "link" | "settle" | "supersede" | "reopen";
export type IntercompanyDispositionState = {
  disposition: IntercompanyDisposition;
  payloadHash: string;
  status: "proposed" | "approved" | "partly_posted" | "posted" | "settled" | "superseded";
  events: IntercompanyDispositionEvent[];
  lifecycleEvents: IntercompanyDispositionEvent[];
  links: Array<{
    side: "left" | "right";
    company_slug: string;
    journal_entry_id: number;
    journal_entry_no: number;
    journal_entry_hash: string;
    ledger_head_hash: string | null;
    linked_at: string;
    actor: string;
    principal_kind: "user" | "service";
    principal_id: string;
  }>;
};
type IntercompanyDispositionEvent = {
  event_type: string;
  actor: string;
  principal_kind: "user" | "service";
  principal_id: string;
  payload_hash: string;
  canonical_payload: string;
  created_at: string;
};
export type IntercompanyDispositionStatus = IntercompanyDispositionState & {
  ok: true;
  scope: "intercompany-disposition-status";
  balanced: boolean;
  exceptions: Array<
    { kind: "stale_or_reversed_journal" | "one_sided_posting"; side?: "left" | "right"; waivable: false } |
    { kind: "overdue_settlement"; dueDate: string; waivable: false }
  >;
};
export type IntercompanyDispositionActionResponse<Action extends IntercompanyDispositionAction> =
  Action extends "plan" ? IntercompanyDispositionPlan : { ok: true } & IntercompanyDispositionState;
