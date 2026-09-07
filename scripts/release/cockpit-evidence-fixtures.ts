/**
 * Versioned, deliberately small DTO corpus used only by the isolated Cockpit
 * browser-evidence runner.  These are protocol-shaped responses, not partial
 * objects: a successful interception must exercise the same renderer path as
 * a real server response.
 */
export const COCKPIT_EVIDENCE_FIXTURE_VERSION = 1;

const company = { name: "Synthetic Evidence Fixture", cvr: "12345678", country: "DK", currency: "DKK", fiscalYearStartMonth: 1, fiscalYearLabelStrategy: "end-year" };
const years = [{ label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" }];
const coverage = { kind: "current", label: "Aktuel bogføring", asOfDate: "2026-06-30", comparison: "available", provenance: "native", details: [] };
const error = (status: 403 | 500) => JSON.stringify({ ok: false, code: status === 403 ? "forbidden" : "internal", errors: [status === 403 ? "Adgang forbudt til evidensvisningen." : "Evidensvisningen kunne ikke hentes."] });

function success(issue: number, empty = false) {
  const rows = empty ? [] : [{ id: 1, entryNo: "B-2026-0001", date: "2026-01-15", text: "Syntetisk journalpost", total: 125, lines: [{ journalLineId: 1, accountNo: "1000", accountName: "Omsætning", debit: 0, credit: 125, text: "Salg" }], documentId: null, documentNo: null }];
  const body: Record<number, unknown> = {
    649: { ok: true, attention: { slug: "evidence-fixture", company: { name: company.name, currency: "DKK" }, scope: { from: "2026-01-01", to: "2026-12-31" }, count: empty ? 0 : 1, items: empty ? [] : [{ id: "evidence-task", severity: "medium", title: "Syntetisk opgave", reason: "Kræver gennemgang", source: "workbench", sourceIdentity: "synthetic", actor: "system", evidence: { fixture: true }, destination: "batchbogfoering" }] } },
    650: { ok: true, fiscalYears: { slug: "evidence-fixture", years } },
    651: { ok: true, changes: { cursor: 1, events: empty ? [] : [{ id: 1, eventType: "bookkeeping", entityType: "journal", entityId: "1", message: "Syntetisk ændring", actor: "system", createdAt: "2026-01-15T00:00:00.000Z" }] } },
    652: { ok: true, journal: { slug: "evidence-fixture", selectedYear: "2026", archived: false, archivedSource: null, company, fiscalYears: years, periodStart: "2026-01-01", periodEnd: "2026-12-31", entries: rows, accountFilter: null } },
    653: { rows: empty ? [] : [{ partyId: "party-evidence", name: "Syntetisk part", roles: ["vendor"], recentActivity: "2026-01-15", computedSpend: 125, partyLink: { href: "/companies/evidence-fixture/parter/party-evidence" } }] },
    654: { ok: true, balance: { slug: "evidence-fixture", selectedYear: "2026", archived: false, archivedSource: null, company, fiscalYears: years, asOfDate: "2026-12-31", assets: { lines: empty ? [] : [{ accountNo: "55000", name: "Bank", amount: 125, priorAmount: 0 }], total: empty ? 0 : 125, priorTotal: 0 }, liabilities: { lines: [], total: 0, priorTotal: 0 }, equity: { lines: empty ? [] : [{ accountNo: "51000", name: "Egenkapital", amount: 125, priorAmount: 0 }], total: empty ? 0 : 125, priorTotal: 0 }, periodResult: 0, totalAssets: empty ? 0 : 125, totalLiabilitiesAndEquity: empty ? 0 : 125, priorTotalLiabilitiesAndEquity: 0, balanced: true, coverage } },
    655: { ok: true, bank: { slug: "evidence-fixture", selectedYear: "2026", archived: false, company, fiscalYears: years, periodStart: "2026-01-01", periodEnd: "2026-12-31", accounts: [{ id: 1, name: "Syntetisk bank", bankName: "Synthetic Bank", accountNo: "5678901234", ledgerAccountNo: "55000" }], bookedBalance: 125, actualBalance: 125, difference: 0, bankStatementStatus: "known", transactions: empty ? [] : [{ id: 1, date: "2026-01-15", text: "Indbetaling faktura 1001", amount: 125, runningBalance: 125, reconciliationStatus: "matched", journalEntryNo: "B-2026-0001" }], matchedCount: empty ? 0 : 1, unmatchedCount: 0 } },
    656: { ok: true, vat: { slug: "evidence-fixture", selectedYear: "2026", archived: false, company, fiscalYears: years, vatRegistered: true, periodStart: "2026-01-01", periodEnd: "2026-03-31", periodLabel: "Q1 2026", outputVat: 25, outputVatAdjustment: 0, inputVat: 0, payable: 25, deadline: "2026-06-01", daysRemaining: 30, periodStatus: "open", momsangivelseReady: false, vatReportErrors: empty ? [] : [], vatReportWarnings: [], rubrikker: { salgsmoms: 25, kobsmoms: 0, momsAfVarekobUdland: 0, momsAfYdelseskobUdland: 0, rubrikAVarer: 0, rubrikAYdelser: 0, rubrikBVarerEuSalesList: 0, rubrikBVarerIkkeEuSalesList: 0, rubrikBYdelser: 0, rubrikC: 0, olieOgFlaskegasafgift: 0, elafgift: 0, naturgasOgBygasafgift: 0, kulafgift: 0, co2Afgift: 0, vandafgift: 0, momsIAlt: 25, wholeKronerDifferenceDkk: 0 } } },
    657: { ok: true, workspace: "/workspace", count: 1, companies: [{ slug: "evidence-fixture", name: company.name, createdAt: "2026-01-01T00:00:00.000Z", archived: false }] },
  };
  return JSON.stringify(body[issue]);
}

export function evidenceResponse(issue: number, state: "normal" | "loading" | "empty" | "warning-or-blocked" | "error") {
  if (state === "warning-or-blocked") return error(403);
  if (state === "error") return error(500);
  return success(issue, state === "empty");
}

type EvidenceState = "normal" | "loading" | "empty" | "warning-or-blocked" | "error";
type EvidenceRequest = { urlPattern: string; status: 200 | 403 | 500; body: string; delayMs?: number };

const batchWorkbench = (empty: boolean) => JSON.stringify({ ok: true, workbench: {
  state: empty ? "zero" : "available", counts: { ready: empty ? 0 : 1, suggestedMatch: 0, missingDocument: 0, partyUnresolved: 0, accountingDecisionRequired: 0, vatEvidenceRequired: 0, dimensionEvidenceRequired: 0, stalePlan: 0, applyFailed: 0 },
  population: { total: empty ? 0 : 1, ready: empty ? 0 : 1, blockers: 0 }, selection: { total: empty ? 0 : 1, ready: empty ? 0 : 1, blockers: 0 }, page: { cursor: 0, total: empty ? 0 : 1, nextCursor: null }, completeness: { nextAction: "Gennemgå den syntetiske bankpost." }, periodClose: { status: "available", blockers: 0 },
  plan: { planHash: "synthetic-plan-hash", candidateSetHash: "synthetic-candidates", readyCount: empty ? 0 : 1 }, rows: empty ? [] : [{ bankTransactionId: 1, date: "2026-01-15", text: "Syntetisk bankpost", amount: 125, currency: "DKK", bankAccount: { id: 1, name: "Syntetisk bank" }, document: null, proposed: { account: "55000", vatTreatment: "none", dimensions: [] }, status: "ready", nextAction: "Forhåndsvis planen.", drilldown: { bankTransactionId: 1, periodClose: { from: "2026-01-01", to: "2026-12-31" } }, sourceHash: "synthetic-source-hash" }]
} });
const batchPlan = JSON.stringify({ ok: true, dryRun: true, plan: { planHash: "synthetic-plan-hash", candidateSetHash: "synthetic-candidates", items: [{ actionKey: "synthetic-action", partition: "2026" }] } });

/** The runner uses this finite list verbatim; it never installs a wildcard route. */
export function evidenceRequests(issue: number, state: EvidenceState): EvidenceRequest[] {
  const primary: EvidenceRequest = { urlPattern: issue === 650 ? "/api/companies/evidence-fixture/fiscal-years" : ({ 649: "/api/companies/evidence-fixture/attention", 651: "/api/companies/evidence-fixture/changes-since?after=0", 652: "/api/companies/evidence-fixture/journal", 653: "/api/companies/evidence-fixture/party-hub?query=", 654: "/api/companies/evidence-fixture/balance", 655: "/api/companies/evidence-fixture/bank", 656: "/api/companies/evidence-fixture/vat", 657: "/api/companies" } as Record<number, string>)[issue]!, status: state === "warning-or-blocked" ? 403 : state === "error" ? 500 : 200, body: evidenceResponse(issue, state), ...(state === "loading" ? { delayMs: 3000 } : {}) };
  if (issue !== 650 || state === "loading" || state === "warning-or-blocked" || state === "error") return [primary];
  return [
    { ...primary, body: evidenceResponse(650, state) },
    { urlPattern: "/api/companies/evidence-fixture/bookkeeping-workbench?from=2026-01-01&to=2026-12-31&cursor=0&limit=25", status: 200, body: batchWorkbench(state === "empty") },
    { urlPattern: "/api/companies/evidence-fixture/bookkeeping-batch?companyId=1&accountingFrom=2026-01-01&accountingTo=2026-12-31&bankFrom=2026-01-01&bankTo=2026-12-31", status: 200, body: batchPlan },
  ];
}

/** Lightweight contract check used by release/unit tests before Chrome runs. */
export function validateEvidenceFixtures() {
  for (const issue of [649, 650, 651, 652, 653, 654, 655, 656, 657]) {
    for (const state of ["normal", "empty", "loading"] as const) {
      const value = JSON.parse(evidenceResponse(issue, state));
      if (issue === 653 ? !Array.isArray(value.rows) : value.ok !== true)
        throw new Error(`invalid success fixture for #${issue}/${state}`);
    }
  }
  for (const state of ["normal", "empty"] as const) {
    const requests = evidenceRequests(650, state);
    if (requests.length !== 3 || requests.some((request) => request.urlPattern.includes("*")))
      throw new Error("#650 must declare its three exact owned requests");
  }
}
