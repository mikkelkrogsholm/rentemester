/**
 * The daily attention inbox (#649).
 *
 * This is deliberately a read-only projection.  Exceptions, period-close
 * controls and the bookkeeping workbench remain the respective sources of
 * truth; this module neither records a task nor changes their completion.
 */
import { Database } from "bun:sqlite";
import { companyPaths } from "../../core/paths";
import { getCompanySettings } from "../../core/company";
import { fiscalYearForDate } from "../../core/fiscal-year";
import { computePeriodCloseReadiness } from "../../core/period-close-readiness";
import { buildBookkeepingWorkbench } from "../../core/bookkeeping-workbench";
import { companyRootForSlug, findWorkspaceCompany } from "../../core/workspace";
import { ApiError } from "../errors";
import { todayIsoDate } from "./shared";

export type AttentionItem = {
  id: string;
  source: "exception" | "agent-proposal" | "readiness" | "workbench";
  severity: "high" | "medium" | "low";
  title: string;
  reason: string;
  actor: string | null;
  destination: string;
  sourceIdentity: string;
  evidence: unknown;
};

const rank: Record<AttentionItem["severity"], number> = { high: 0, medium: 1, low: 2 };
const workbenchSeverity: Record<string, AttentionItem["severity"]> = {
  applyFailed: "high", stalePlan: "high", missingDocument: "medium", partyUnresolved: "medium",
  accountingDecisionRequired: "medium", vatEvidenceRequired: "high", dimensionEvidenceRequired: "medium",
  suggestedMatch: "low",
};
const workbenchReason: Record<string, string> = {
  suggestedMatch: "Gennemgå det foreslåede match.", missingDocument: "Find eller tilknyt det manglende bilag.",
  partyUnresolved: "Afklar den relevante part.", accountingDecisionRequired: "Afklar konto og bogføringsregel.",
  vatEvidenceRequired: "Tilføj grundlag for momsbehandlingen.", dimensionEvidenceRequired: "Afklar den nødvendige dimension.",
  stalePlan: "Lav en ny plan på det aktuelle grundlag.", applyFailed: "Gennemgå den tidligere bogføringskørsel.",
};

function exceptionDestination(type: string): string {
  if (type.startsWith("AGENT_PAYABLE")) return "leverandoerfaktura";
  if (type.startsWith("AGENT_ACCRUAL")) return "posteringer";
  if (type.startsWith("AGENT_POSSIBLE_FIXED_ASSET")) return "anlaeg";
  if (type.includes("BANK")) return "bank";
  if (type.includes("DOCUMENT")) return "bilag";
  return "undtagelser";
}

/** A deterministic aggregate of currently actionable canonical records. */
export function buildCompanyAttention(workspaceRoot: string, slug: string) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  const db = new Database(companyPaths(companyRootForSlug(workspaceRoot, slug)).db, { readonly: true });
  try {
    const company = getCompanySettings(db);
    const today = todayIsoDate();
    const fiscalYear = fiscalYearForDate(today, Number(company.fiscalYearStartMonth), company.fiscalYearLabelStrategy);
    const items: AttentionItem[] = [];
    const exceptions = db.query(`SELECT e.id,e.type,e.severity,e.message,e.required_action,e.source_evidence,e.created_at,e.related_bank_transaction_id,
      (SELECT actor FROM audit_log WHERE entity_type='exception' AND entity_id=CAST(e.id AS TEXT) ORDER BY id DESC LIMIT 1) actor
      FROM exceptions e WHERE e.status='open' ORDER BY e.id DESC`).all() as Array<any>;
    const exceptionBankTransactions = new Set<number>();
    for (const row of exceptions) {
      if (typeof row.related_bank_transaction_id === "number") exceptionBankTransactions.add(row.related_bank_transaction_id);
      const agent = String(row.type).startsWith("AGENT_");
      // Agent suggestions are projections of these exact exception records;
      // classify them here instead of adding the suggestions projection again.
      items.push({
        id: `exception:${row.id}`, source: agent ? "agent-proposal" : "exception",
        severity: row.severity === "high" || row.severity === "medium" ? row.severity : "low",
        title: agent ? "Agentforslag kræver stillingtagen" : "Kræver opmærksomhed",
        reason: row.required_action || row.message, actor: row.actor ?? null,
        destination: exceptionDestination(row.type), sourceIdentity: `exception:${row.id}`,
        evidence: { type: row.type, message: row.message, createdAt: row.created_at, sourceEvidence: safeJson(row.source_evidence) },
      });
    }
    const readiness = computePeriodCloseReadiness(db, { periodStart: fiscalYear.start, periodEnd: fiscalYear.end });
    for (const control of readiness.items.filter((item) => item.status === "blocked")) items.push({
      id: `readiness:${control.code}:${control.sourceHash}`, source: "readiness", severity: "high",
      title: "Regnskabsperioden kan ikke lukkes", reason: "Gennemgå det manglende lukkegrundlag.",
      actor: null, destination: `periodelas?from=${fiscalYear.start}&to=${fiscalYear.end}`,
      sourceIdentity: `readiness:${control.code}:${control.sourceHash}`, evidence: control,
    });
    const workbench = buildBookkeepingWorkbench(db, { from: fiscalYear.start, to: fiscalYear.end, limit: 100 });
    for (const row of workbench.rows.filter((candidate) => candidate.status !== "ready" && !exceptionBankTransactions.has(candidate.bankTransactionId))) items.push({
      id: `workbench:${row.bankTransactionId}`, source: "workbench", severity: workbenchSeverity[row.status] ?? "medium",
      title: "Bogføring kræver afklaring", reason: workbenchReason[row.status] ?? "Gennemgå bogføringsgrundlaget.", actor: null,
      destination: `batchbogfoering?from=${fiscalYear.start}&to=${fiscalYear.end}&search=${encodeURIComponent(String(row.bankTransactionId))}`,
      sourceIdentity: `workbench:${row.bankTransactionId}`, evidence: { status: row.status, sourceHash: row.sourceHash, drilldown: row.drilldown },
    });
    const unique = [...new Map(items.map((item) => [item.sourceIdentity, item])).values()];
    unique.sort((a, b) => rank[a.severity] - rank[b.severity] || a.id.localeCompare(b.id));
    return {
      slug: entry.slug,
      company: { name: company.name, currency: company.currency },
      scope: { from: fiscalYear.start, to: fiscalYear.end },
      items: unique,
      count: unique.length,
      status: unique.length > 0 ? "requires-attention" as const : "clear" as const,
    };
  } finally { db.close(); }
}

function safeJson(raw: string | null): unknown { try { return raw ? JSON.parse(raw) : null; } catch { return raw; } }
