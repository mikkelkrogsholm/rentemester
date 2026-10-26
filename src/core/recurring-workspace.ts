/** Explicit scheduler entry point for recurring invoices across a workspace.
 *
 * It deliberately owns neither a timer nor provider credentials. Production
 * scheduling calls this function (normally once daily) with an injected,
 * already-configured delivery adapter. The manifest is the only company list.
 */
import { existsSync } from "node:fs";
import { openDb, migrate } from "./db";
import { openLedgerReadOnly } from "./ledger-inspection";
import { evaluateBackupLock } from "./backup-governance";
import { companyPaths } from "./paths";
import { companyRootForSlug, listWorkspaceCompanies } from "./workspace";
import { checkActorAllowlist, isCanonicalActorId } from "../cli-actor";
import type { ResolveActorInput } from "./actor";
import { runRecurringInvoices, type RecurringDeliveryAdapter } from "./recurring-runner";
import { resolveRecurringDeliveryAdapter } from "./recurring-delivery-adapter";

export type WorkspaceRecurringCompanyResult = {
  slug: string;
  status: "processed" | "skipped" | "failed";
  reason?: "archived" | "not initialized" | "run failed";
  generated?: number;
  attempted?: number;
  hasMore?: boolean;
  remainingGenerations?: number;
  continuation?: { remainingGenerations: number };
};
export type WorkspaceRecurringRunResult = {
  ok: boolean;
  companies: WorkspaceRecurringCompanyResult[];
  errors: string[];
  hasMore: boolean;
  remainingGenerations: number;
  continuation?: {
    remainingGenerations: number;
    companies: Array<{ slug: string; remainingGenerations: number }>;
  };
};
export type WorkspaceRecurringRunOptions = {
  asOfDate: string;
  actor: Required<Pick<ResolveActorInput, "createdBy" | "createdByProgram">>;
  maxGenerations?: number;
  /** Test/operations seam. It is never resolved from a workspace manifest. */
  adapterForCompany?: (input: { db: ReturnType<typeof openDb>; companyRoot: string; slug: string }) => RecurringDeliveryAdapter | undefined;
  runCompany?: (input: { db: ReturnType<typeof openDb>; companyRoot: string; slug: string }) => Promise<{ ok: boolean; generated: number; attempted: number; hasMore?: boolean; remainingGenerations?: number; continuation?: { remainingGenerations: number } }> | { ok: boolean; generated: number; attempted: number; hasMore?: boolean; remainingGenerations?: number; continuation?: { remainingGenerations: number } };
};
type Target = { slug: string; root: string };

function targets(workspaceRoot: string): { active: Target[]; skipped: WorkspaceRecurringCompanyResult[] } {
  const active: Target[] = [];
  const skipped: WorkspaceRecurringCompanyResult[] = [];
  for (const entry of [...listWorkspaceCompanies(workspaceRoot)].sort((a, b) => a.slug.localeCompare(b.slug))) {
    if (entry.archived) { skipped.push({ slug: entry.slug, status: "skipped", reason: "archived" }); continue; }
    const root = companyRootForSlug(workspaceRoot, entry.slug);
    if (!existsSync(companyPaths(root).db)) { skipped.push({ slug: entry.slug, status: "skipped", reason: "not initialized" }); continue; }
    active.push({ slug: entry.slug, root });
  }
  return { active, skipped };
}

function preflightTarget(target: Target, actor: string): boolean {
  if (!isCanonicalActorId(actor)) return false;
  const allowed = checkActorAllowlist(target.root, actor);
  if (!allowed.allowed) return false;
  const db = openLedgerReadOnly(companyPaths(target.root).db);
  try {
    db.exec("PRAGMA query_only = ON; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 30000;");
    const lock = evaluateBackupLock(db, target.root);
    return lock.errors.length === 0 && !lock.locked;
  } catch { return false; }
  finally { db.close(); }
}

/** Runs every active, initialized manifest company in stable slug order. */
export async function runWorkspaceRecurringInvoices(workspaceRoot: string, options: WorkspaceRecurringRunOptions): Promise<WorkspaceRecurringRunResult> {
  const selected = targets(workspaceRoot);
  const companies: WorkspaceRecurringCompanyResult[] = [...selected.skipped];
  for (const target of selected.active) {
    // A bad policy/backup state is confined to this company; the next
    // manifest target still runs. No database is opened read-write first.
    if (!preflightTarget(target, options.actor.createdBy ?? "")) {
      companies.push({ slug: target.slug, status: "failed", reason: "run failed" });
      continue;
    }
    const db = openDb(companyPaths(target.root).db);
    try {
      migrate(db);
      const result = options.runCompany
        ? await options.runCompany({ db, companyRoot: target.root, slug: target.slug })
        : await runRecurringInvoices(db, {
            companyRoot: target.root,
            asOfDate: options.asOfDate,
            adapter: options.adapterForCompany
              ? options.adapterForCompany({ db, companyRoot: target.root, slug: target.slug })
              : resolveRecurringDeliveryAdapter(db, target.root),
            createdBy: options.actor.createdBy ?? undefined,
            createdByProgram: options.actor.createdByProgram ?? undefined,
            maxGenerations: options.maxGenerations,
          });
      companies.push(result.ok
        ? { slug: target.slug, status: "processed", generated: result.generated, attempted: result.attempted, hasMore: result.hasMore ?? false, remainingGenerations: result.remainingGenerations ?? 0, ...(result.continuation ? { continuation: result.continuation } : {}) }
        : { slug: target.slug, status: "failed", reason: "run failed", generated: result.generated, attempted: result.attempted, hasMore: result.hasMore ?? false, remainingGenerations: result.remainingGenerations ?? 0, ...(result.continuation ? { continuation: result.continuation } : {}) });
    } catch { companies.push({ slug: target.slug, status: "failed", reason: "run failed" }); }
    finally { db.close(); }
  }
  const failed = companies.filter((company) => company.status === "failed");
  const continuations = companies
    .filter((company) => company.hasMore && (company.remainingGenerations ?? 0) > 0)
    .map((company) => ({ slug: company.slug, remainingGenerations: company.remainingGenerations! }));
  const remainingGenerations = continuations.reduce((sum, company) => sum + company.remainingGenerations, 0);
  return {
    ok: failed.length === 0,
    companies,
    errors: failed.map((company) => `recurring run failed for company ${company.slug}`),
    hasMore: remainingGenerations > 0,
    remainingGenerations,
    ...(remainingGenerations > 0 ? { continuation: { remainingGenerations, companies: continuations } } : {}),
  };
}
