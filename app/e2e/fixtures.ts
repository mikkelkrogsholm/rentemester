import { expect, type Page, type Route } from "@playwright/test";
import core from "./data/core.json" with { type: "json" };
import type { CompanyRole } from "../../src/core/workspace-access";
import type { CompanySummary } from "../src/lib/types";

export const COMPANY_SLUG = "acme-aps";
export const COMPANY_NAME = "Acme ApS";
const companyEntries = [
  { slug: COMPANY_SLUG, name: COMPANY_NAME, createdAt: "2026-01-01T00:00:00Z", archived: false },
  { slug: "beta-aps", name: "Beta ApS", createdAt: "2026-01-01T00:00:00Z", archived: false },
];
const user = { id: "synthetic-owner", name: "Testbruger", email: "owner@example.invalid", emailVerified: true, twoFactorEnabled: true };
const { assets, bank, budget, budgetDimensionActuals, budgetVsActual, cashflow, archive, companySettings, fiscalYears, multiYear, contacts, overview, documents, invoices, mileage, obligations, payables, balance, incomeStatement, journal, trialBalance, vat } = Object.fromEntries(Object.entries(core).map(([key, value]) => [key, () => value])) as { [K in keyof typeof core]: () => (typeof core)[K] };
const STATEMENT_COMPANY = core.company;
const summary = (overrides: Partial<CompanySummary> = {}): CompanySummary => ({ ...core.summary, ...overrides });

export type MockResponse = { body: unknown; status?: number; contentType?: string; delayMs?: number };
export type FixtureOptions = {
  profile?: "local" | "local-container" | "hosted";
  role?: CompanyRole;
  workspaceRole?: "workspace_owner" | "member";
  companyAccess?: boolean;
  session?: "ready" | "absent" | "unverified" | "mfa-required";
  overrides?: Record<string, MockResponse | ((route: Route) => Promise<MockResponse>)>;
};

function envelope(key: string, body: unknown, url: URL) {
  if (typeof body !== "object" || body === null) return { ok: true, [key]: body };
  const value = body as Record<string, unknown>;
  const slug = url.pathname.match(/^\/api\/companies\/([^/]+)/)?.[1] ?? COMPANY_SLUG;
  const year = url.searchParams.get("year") ?? "2026";
  return { ok: true, [key]: {
    ...value,
    ...(Object.hasOwn(value, "slug") ? { slug } : {}),
    ...(Object.hasOwn(value, "company") && slug === "beta-aps" ? { company: { ...(value.company as object), name: "Beta ApS" } } : {}),
    ...(Object.hasOwn(value, "selectedYear") ? { selectedYear: year, archived: year === "2025" } : {}),
  } };
}

function syntheticVoucherPdf(): string {
  const stream = "BT /F1 12 Tf 50 740 Td (Synthetic voucher - 1250 DKK) Tj ET";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    `<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`,
  ];
  let pdf = "%PDF-1.4\n";
  const offsets = [0];
  objects.forEach((object, index) => {
    offsets.push(pdf.length);
    pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;
  });
  const xref = pdf.length;
  pdf += `xref\n0 ${offsets.length}\n0000000000 65535 f \n`;
  for (const offset of offsets.slice(1)) pdf += `${String(offset).padStart(10, "0")} 00000 n \n`;
  return pdf + `trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
}

/** Explicit success responses. Unknown endpoints are never given a generic OK. */
function fixtureFor(url: URL, options: FixtureOptions): MockResponse | undefined {
  const path = url.pathname.replace(/^\/api\/companies\/[^/]+\//, "");
  const wrap = (key: string, value: unknown): MockResponse => ({ body: envelope(key, value, url) });
  const company = { slug: COMPANY_SLUG, company: STATEMENT_COMPANY };
  switch (path) {
    case "/api/health": return { body: { ok: true, deploymentProfile: options.profile ?? "local", workspace: "synthetic-browser-workspace", version: "browser-fixture", companyCount: 2 } };
    case "/api/companies": return { body: { ok: true, workspace: "synthetic-browser-workspace", count: options.companyAccess === false ? 0 : companyEntries.length, companies: options.companyAccess === false ? [] : companyEntries } };
    case "/api/portfolio": return wrap("portfolio", {
      workspace: "synthetic-browser-workspace", asOf: "2026-05-20", companyCount: 2,
      rollup: { resultat: 13234.82, liquidity: 23654.75, vatPayable: -200, openTaskCount: 1 },
      totals: { openInvoiceCount: 1, openInvoiceTotal: 6250, overdueInvoiceCount: 1, unlinkedBankCount: 1, openExceptionCount: 0, netVatPayable: -200 },
      companies: [summary({ actualBankBalance: null, vat: { payable: -200, deadline: "2026-09-01", daysRemaining: 103 }, openTaskCount: 1, taskGroups: [{ type: "bank", label: "Bankpost mangler bilag", count: 1, severity: "medium", link: "bank?status=unmatched" }] }), summary({ slug: "beta-aps", name: "Beta ApS", ledgerMissing: true, actualBankBalance: null, vat: null })],
    });
    case "/api/system/cvr":
    case "/api/system/cvr-status":
    case "/api/cvr/status": return { body: { ok: true, cvrStatus: { configured: false }, configured: false, enabled: false, provider: null } };
    case "/api/rules": return { body: { ok: true, ruleBundles: [{ name: "Syntetiske regler", version: "1", ruleCount: 0, sources: [], vatCodes: [] }], rules: [], legalSources: [] } };
    case "/api/auth/get-session": {
      if (options.session === "absent") return { body: null };
      return { body: { session: { id: "synthetic-session", userId: user.id, expiresAt: "2099-01-01T00:00:00Z", createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z" }, user: { ...user, emailVerified: options.session !== "unverified", twoFactorEnabled: options.session !== "mfa-required" } } };
    }
    case "/api/me": return { body: { ok: true, user, workspaceRole: options.workspaceRole ?? "workspace_owner", companies: options.companyAccess === false ? [] : companyEntries.map((entry) => ({ ...entry, role: options.role ?? "owner" })) } };
    case "/api/workspace/invitations": return { body: { ok: true, invitations: [] } };
    case "/api/workspace/members": return { body: { ok: true, members: [] } };
    case "/api/auth/list-sessions": return { body: [] };
    case "/api/group-overview": return { body: { ok: true, scope: "structure-status-only", consolidationStatus: "not-available", consolidatedFigures: null, rawCompanySums: null, blockers: [], manifestStatus: "ready", asOf: url.searchParams.get("asOf"), groups: [] } };
    case "/api/group-reconciliation": return { body: { ok: true, scope: "intercompany-reconciliation", asOf: url.searchParams.get("asOf"), rows: [] } };
    case "/api/group-eliminations": return { body: { ok: true, scope: "consolidation-eliminations", asOf: url.searchParams.get("asOf"), rows: [] } };
    case "/api/group-report-profiles": return { body: { ok: true, scope: "consolidation-report-profiles", asOf: url.searchParams.get("asOf"), profiles: [] } };
    case "/api/cfo-analytics": return { body: { ok: true, schemaVersion: "1", scope: "company", status: "ready", asOf: url.searchParams.get("to"), from: url.searchParams.get("from"), to: url.searchParams.get("to"), companies: [COMPANY_SLUG], partial: false, mode: "legal-company", aggregate: "none", limitations: [], rows: [], page: { limit: 50, nextCursor: null }, freshness: [], evidenceCompleteness: [], reconciliation: { rowCount: 0, sourceHashes: [], method: "source-linked" } } };
    case "overview": return wrap("overview", overview());
    case "fiscal-years": return wrap("fiscalYears", fiscalYears());
    case "income-statement": return wrap("incomeStatement", incomeStatement());
    case "balance": return wrap("balance", balance());
    case "trial-balance": return wrap("trialBalance", trialBalance());
    case "journal": return wrap("journal", journal());
    case "obligations": return wrap("obligations", obligations());
    case "cashflow": return wrap("cashflow", cashflow());
    case "multi-year": return wrap("multiYear", multiYear());
    case "archive/2025": return wrap("archive", archive());
    case "bank": return wrap("bank", bank());
    case "documents": return wrap("documents", documents());
    case "documents/1/file": return { contentType: "application/pdf", body: syntheticVoucherPdf() };
    case "documents/party-links": return { body: { ok: true, links: [{ id: 1, document_no: "DOC-2026-000001", linked: 1, resolution_state: "resolved" }] } };
    case "documents/1/party-links": return { body: { ok: true, links: [] } };
    case "invoices": return wrap("invoices", invoices());
    case "recurring-invoices": return wrap("recurringInvoices", { ...company, templates: [], generations: [] });
    case "contacts": return wrap("contacts", contacts());
    case "company": return { body: { ok: true, company: companySettings() } };
    case "vat": return wrap("vat", vat());
    case "mileage": return wrap("mileage", mileage());
    case "assets": return wrap("assets", assets());
    case "payables": return wrap("payables", payables());
    case "budget": return wrap("budget", budget());
    case "budget-vs-actual": return wrap("budgetVsActual", budgetVsActual());
    case "budget-dimension-actuals": return wrap("budgetDimensionActuals", budgetDimensionActuals());
    case "supplier-commitments": return wrap("supplierCommitments", { commitments: [], alerts: [], matches: [], forecast: { ok: true, openingCash: 0, lowestPoint: 0, periods: [], completeness: { included: [], excluded: [] } } });
    case "accounting-drafts": return { body: { ok: true, accountingDrafts: [] } };
    case "posting-rules": return { body: { ok: true, postingRules: [] } };
    case "agent-suggestions": return wrap("agentSuggestions", { ...company, rows: [], count: 0, bySeverity: { high: 0, medium: 0, low: 0 } });
    case "exceptions": return wrap("exceptions", { ...company, status: "open", rows: [], count: 0, bySeverity: { high: 0, medium: 0, low: 0 } });
    case "dimensions": return { body: { ok: true, definitions: [] } };
    case "dimensions/members": return { body: { ok: true, members: [] } };
    case "workspace-parties": return { body: { ok: true, rows: [], count: 0 } };
    case "corporate-records": return { body: { ok: true, rows: [], count: 0 } };
    case "knowledge": return { body: { ok: true, context: { assertions: [], conflicts: [] } } };
    case "ownership": return { body: { ok: true, asOf: url.searchParams.get("asOf"), facts: [], partial: false, consolidation: { eligible: false, reason: "Ingen godkendte ejerforhold." } } };
    case "ownership/history": return { body: { ok: true, history: [] } };
    case "workspace-inbox": return { body: { ok: true, rows: [] } };
    case "accounts": return wrap("accounts", { ...company, accounts: [ { accountNo: "3000", name: "Kontorartikler", type: "expense", normalBalance: "debit", defaultVatCode: "I25", hasPostings: true } ], byType: { expense: 1 }, accountRoles: { status: "incomplete", missing: ["bank"], ambiguous: [], resolutions: [], proposals: [], candidates: [], reasons: [] } });
    case "periods": return wrap("periods", { ...company, periods: [], byStatus: { open: 0, closed: 0, reported: 0 } });
    case "retention": return wrap("retention", { ...company, report: { ok: true, asOf: "2026-05-20", appliedRules: [], rows: [{ table: "documents", total: 1, expired: 0, nextExpiry: "2031-12-31", oldestExpired: null }], errors: [] }, legalCitation: { sourceId: "synthetic-retention", note: "Syntetisk testgrundlag." } });
    case "integrity": return wrap("integrity", { ...company, auditChain: { ok: true, entries: 1, errors: [] }, backup: { ok: true, latestBackupAt: "2026-05-20T00:00:00Z", latestBackupId: "synthetic-backup", backupDue: false, hasActivitySinceBackup: false, daysSinceLatestBackup: 0, backupsFound: 1, requiredBy: null, checkedAt: "2026-05-20T00:00:00Z" }, destinations: [], legalCitation: { sourceId: "synthetic-backup", note: "Syntetisk testgrundlag." } });
    case "bank-accounts": return wrap("bankAccounts", { ...company, accounts: [], profiles: [{ name: "synthetic-csv", bankName: "Testbank", separator: ";", encoding: "utf-8", dateOrder: "dmy" }] });
    case "accruals": return wrap("accruals", { ...company, report: { ok: true, accruals: [], totals: { totalAmount: 0, recognizedAmount: 0, remainingAmount: 0 }, errors: [] } });
    case "bilagsmail": return wrap("bilagsmail", { ...company, imapConfigured: false, imapStatus: null, mailAlias: null, inbox: [] });
    default: return undefined;
  }
}

export async function mockApi(page: Page, options: FixtureOptions = {}) {
  const unexpected: string[] = [];
  const calls: Array<{ method: string; path: string; body: unknown }> = [];
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const method = request.method();
    const key = `${method} ${url.pathname}`;
    calls.push({ method, path: url.pathname, body: request.postDataJSON() });
    const override = options.overrides?.[key];
    const response = override ? (typeof override === "function" ? await override(route) : override) : method === "GET" ? fixtureFor(url, options) : undefined;
    if (!response) {
      unexpected.push(key);
      await route.fulfill({ status: 503, json: { ok: false, code: "fixture_required", errors: [`No synthetic fixture for ${key}`] } });
      return;
    }
    if (response.delayMs) await new Promise((resolve) => setTimeout(resolve, response.delayMs));
    if (response.contentType) await route.fulfill({ status: response.status ?? 200, contentType: response.contentType, body: String(response.body) });
    else await route.fulfill({ status: response.status ?? 200, json: response.body });
  });
  return { calls, unexpected, assertComplete: () => expect(unexpected, "Every API read or mutation needs an explicit synthetic fixture").toEqual([]) };
}
