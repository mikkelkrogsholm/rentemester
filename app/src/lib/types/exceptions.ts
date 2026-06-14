// Exceptions queue (#332) + agent-suggestion approval wire types (#346).

import type { StatementCompany } from "./common";

/**
 * Slim exception shape returned by `CompanyDashboard.exceptions.rows`
 * (`/api/companies/:slug/dashboard`). Mirrors the 5-field projection in
 * `src/server/data/portfolio.ts` (buildCompanyDashboardData). The richer
 * Exceptions-queue payload uses `ExceptionRow` below.
 */
export type DashboardExceptionRow = {
  id: number;
  type: string;
  severity: string;
  status: string;
  message: string;
};

// ---------------------------------------------------------------------------
// Agent-forslag → menneskelig godkendelse (#346)
// ---------------------------------------------------------------------------

/**
 * One agent suggestion waiting on the owner's approve/reject decision. Mirrors
 * `AgentSuggestionRow` in `src/server/data/agent-suggestions.ts` — the cockpit
 * never re-derives the rule id, severity or kind label.
 */
export type AgentSuggestionRow = {
  exceptionId: number;
  type: string;
  kindLabel: string;
  severity: "low" | "medium" | "high";
  rationale: string;
  requiredAction: string | null;
  ruleId: string | null;
  sourceEvidence: unknown;
  postingPreview: unknown;
  agentActor: string | null;
  agentProgram: string | null;
  createdAt: string;
  relatedDocumentId: number | null;
  relatedBankTransactionId: number | null;
  /** Cockpit deep-link target ("anlaeg", "leverandoerfaktura", …); may be null. */
  link: string | null;
};

export type CompanyAgentSuggestions = {
  slug: string;
  company: StatementCompany;
  rows: AgentSuggestionRow[];
  count: number;
  bySeverity: {
    high: number;
    medium: number;
    low: number;
  };
};

export type AgentSuggestionsResponse = {
  ok: true;
  agentSuggestions: CompanyAgentSuggestions;
};

/** Result of an approve/reject decision — the resolved-id pair the cockpit echoes. */
export type AgentSuggestionDecisionResult = {
  id: number;
  decision: "approved" | "rejected";
  resolved: boolean;
};

export type AgentSuggestionDecisionResponse = {
  ok: true;
  suggestion: AgentSuggestionDecisionResult;
};

// ---------------------------------------------------------------------------
// #332 — Exceptions queue (read-only liste).
// ---------------------------------------------------------------------------

export type ExceptionRow = {
  id: number;
  type: string;
  severity: "low" | "medium" | "high";
  status: "open" | "resolved";
  relatedBankTransactionId: number | null;
  relatedDocumentId: number | null;
  message: string;
  requiredAction: string | null;
  sourceEvidence: unknown;
  postingPreview: unknown;
  createdAt: string;
  resolvedAt: string | null;
  resolvedBy: string | null;
  resolutionNote: string | null;
  archived: boolean;
};

export type CompanyExceptions = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  status: "open" | "resolved" | "all";
  rows: ExceptionRow[];
  bySeverity: { high: number; medium: number; low: number };
  count: number;
};

export type ExceptionsResponse = {
  ok: true;
  exceptions: CompanyExceptions;
};
