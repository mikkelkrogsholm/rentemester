import { runSql } from "./sqlite";
import type { Database } from "bun:sqlite";
import { readDineroVatRoleReview, type DineroVatRoleReview } from "./import/dinero-vat-role-review";
import { insertAuditLog } from "./actor";

export const ACCOUNT_ROLES = ["bank", "debtors", "creditors", "output_vat", "input_vat", "reverse_charge_vat", "vat_settlement", "operational_default"] as const;
export type AccountRole = typeof ACCOUNT_ROLES[number];
export type AccountRoleStatus = "complete" | "incomplete" | "ambiguous";

const expectations: Record<AccountRole, { type: string; normalBalance: string }> = {
  bank: { type: "asset", normalBalance: "debit" },
  debtors: { type: "asset", normalBalance: "debit" },
  creditors: { type: "liability", normalBalance: "credit" },
  output_vat: { type: "vat", normalBalance: "credit" },
  input_vat: { type: "vat", normalBalance: "debit" },
  reverse_charge_vat: { type: "vat", normalBalance: "credit" },
  vat_settlement: { type: "liability", normalBalance: "credit" },
  operational_default: { type: "expense", normalBalance: "debit" },
};

export type AccountRoleConfirmationSource = "native_seed" | "explicit";
type ReviewedDineroControl = DineroVatRoleReview & {
  lastExistingEntryId: number;
};
export type AccountRoleResolution =
  | { ok: true; role: AccountRole; accountNo: string; version: number; confirmedBy: string; confirmationSource: AccountRoleConfirmationSource; dineroReview?: ReviewedDineroControl }
  | { ok: false; role: AccountRole; error: string };

export type AccountRoleProposal = { role: AccountRole; accountNo: string; source: string; proposedAt: string; compatible: boolean; reason?: string };
export type AccountRoleStatusDetail = {
  status: AccountRoleStatus;
  missing: AccountRole[];
  ambiguous: AccountRole[];
  proposals: AccountRoleProposal[];
  candidates: AccountRoleProposal[];
  reasons: Array<{ role: AccountRole; reason: string }>;
};

/** Read-only posting preflight. Completeness is deliberately the aggregate of
 * the same resolver used by posting code; a stale seed is not a confirmation. */
export function accountRoleStatus(db: Database): AccountRoleStatusDetail {
  const proposals = db.query("SELECT role, account_no, source, proposed_at FROM account_role_proposals WHERE status = 'proposed' ORDER BY role, account_no, source").all() as Array<{ role: AccountRole; account_no: string; source: string; proposed_at: string }>;
  const candidates = proposals.map((row) => {
    const compatibility = accountRoleCompatibility(db, row.role, row.account_no);
    return {
      role: row.role,
      accountNo: row.account_no,
      source: row.source,
      proposedAt: row.proposed_at,
      compatible: compatibility.ok,
      ...(compatibility.ok ? {} : { reason: compatibility.error }),
    };
  });
  const missing: AccountRole[] = [];
  const ambiguous: AccountRole[] = [];
  const reasons: Array<{ role: AccountRole; reason: string }> = [];
  for (const role of ACCOUNT_ROLES) {
    const resolution = resolveAccountRole(db, role);
    if (resolution.ok) continue;
    const distinct = new Set(candidates.filter((proposal) => proposal.role === role && proposal.compatible).map((proposal) => proposal.accountNo));
    if (distinct.size > 1) ambiguous.push(role);
    else missing.push(role);
    reasons.push({ role, reason: resolution.error + (distinct.size ? `; ${distinct.size} unconfirmed proposal${distinct.size === 1 ? "" : "s"}` : "") });
  }
  return { status: ambiguous.length ? "ambiguous" : missing.length ? "incomplete" : "complete", missing, ambiguous, proposals: candidates, candidates, reasons };
}

function matchesReviewedLiability(db: Database, role: AccountRole, accountNo: string, review: Pick<ReviewedDineroControl, "accountName" | "companyCvr">) {
  if (expectations[role].type !== "vat") return false;
  return db.query(`SELECT 1 FROM accounts a JOIN companies c ON c.id = 1
    WHERE a.account_no = ? AND trim(a.name) = ? AND a.active = 1 AND a.type = 'liability'
      AND coalesce(a.default_vat_code, '') = '' AND (? = 'input_vat' OR a.normal_balance = 'credit')
      AND upper(trim(c.cvr)) IN (?, ?)`).get(accountNo, review.accountName, role, review.companyCvr, review.companyCvr.slice(2)) !== null;
}

export function resolveAccountRole(db: Database, role: AccountRole): AccountRoleResolution {
  const row = db.query(`SELECT m.id, m.account_no, m.version, m.confirmed_by, m.confirmation_source, a.type, a.normal_balance, a.active
    FROM account_role_mappings m JOIN accounts a ON a.account_no = m.account_no
    WHERE m.role = ? AND m.status = 'confirmed'`).get(role) as { id: number; account_no: string; version: number; confirmed_by: string; confirmation_source: AccountRoleConfirmationSource; type: string; normal_balance: string; active: number } | null;
  if (!row) return { ok: false, role, error: `account role '${role}' has no active confirmed mapping` };
  if (row.active !== 1) return { ok: false, role, error: `account role '${role}' maps to inactive account ${row.account_no}` };
  const expected = expectations[role];
  let dineroReview: ReviewedDineroControl | null = null;
  if (row.type !== expected.type || row.normal_balance !== expected.normalBalance) {
    if (row.confirmation_source === "explicit" && row.type === "liability" && expected.type === "vat") {
      dineroReview = db.query(`SELECT company_cvr AS companyCvr, chart_path AS chartPath,
        chart_sha256 AS chartSha256, inventory_sha256 AS inventorySha256,
        source_account_name AS accountName, last_existing_entry_id AS lastExistingEntryId
        FROM dinero_vat_role_reviews WHERE mapping_id = ?`).get(row.id) as ReviewedDineroControl | null;
    }
    if (!dineroReview || !matchesReviewedLiability(db, role, row.account_no, dineroReview)) {
      return { ok: false, role, error: `account role '${role}' maps to semantically incompatible account ${row.account_no}` };
    }
  }
  return { ok: true, role, accountNo: row.account_no, version: row.version, confirmedBy: row.confirmed_by,
    confirmationSource: row.confirmation_source, ...(dineroReview ? { dineroReview } : {}) };
}

/** A shared output account needs per-entry base allocation in the VAT report. */
export function dedicatedReverseChargeAccount(db: Database): string | null {
  const output = resolveAccountRole(db, "output_vat");
  const reverse = resolveAccountRole(db, "reverse_charge_vat");
  return reverse.ok && (!output.ok || reverse.accountNo !== output.accountNo) ? reverse.accountNo : null;
}

export function accountRoleCompatibility(db: Database, role: AccountRole, accountNo: string): { ok: true } | { ok: false; error: string } {
  const account = db.query("SELECT type, normal_balance, active FROM accounts WHERE account_no = ?").get(accountNo) as { type: string; normal_balance: string; active: number } | null;
  const expected = expectations[role];
  if (!account) return { ok: false, error: `account ${accountNo} does not exist` };
  if (account.active !== 1) return { ok: false, error: `account ${accountNo} is inactive` };
  if (account.type !== expected.type || account.normal_balance !== expected.normalBalance) {
    if (expected.type === "vat" && account.type === "liability") {
      const confirmed = resolveAccountRole(db, role);
      if (confirmed.ok && confirmed.accountNo === accountNo && confirmed.dineroReview) return { ok: true };
    }
    return { ok: false, error: `account ${accountNo} is not compatible with role '${role}'` };
  }
  return { ok: true };
}

export function proposeAccountRole(db: Database, role: AccountRole, accountNo: string, source: string): boolean {
  const compatibility = accountRoleCompatibility(db, role, accountNo);
  if (!compatibility.ok) return false;
  const result = runSql(db, "INSERT OR IGNORE INTO account_role_proposals (role, account_no, source) VALUES (?, ?, ?)", role, accountNo, source);
  return result.changes > 0;
}

export function persistAccountRoleProposals(db: Database, proposals: ReadonlyArray<{ role: AccountRole; accountNo: string; source: string }>): { stored: number; ignored: number; reviewRequired: AccountRole[] } {
  let stored = 0;
  let ignored = 0;
  const reviewRequired = new Set<AccountRole>();
  const grouped = new Map<AccountRole, Array<{ role: AccountRole; accountNo: string; source: string }>>();
  for (const proposal of proposals) grouped.set(proposal.role, [...(grouped.get(proposal.role) ?? []), proposal]);

  for (const [role, roleProposals] of grouped) {
    const currentRow = db.query("SELECT account_no, confirmation_source FROM account_role_mappings WHERE role = ? AND status = 'confirmed'").get(role) as { account_no: string; confirmation_source: AccountRoleConfirmationSource } | null;
    const current = resolveAccountRole(db, role);
    if (current.ok && current.confirmationSource === "explicit") {
      ignored += roleProposals.length;
      continue;
    }
    const valid = roleProposals.filter((proposal) => accountRoleCompatibility(db, role, proposal.accountNo).ok);
    ignored += roleProposals.length - valid.length;
    const differsFromNative = currentRow?.confirmation_source === "native_seed" && valid.some((proposal) => proposal.accountNo !== currentRow.account_no);
    if (differsFromNative) {
      runSql(db, "UPDATE account_role_mappings SET status = 'inactive' WHERE role = ? AND status = 'confirmed' AND confirmation_source = 'native_seed'", role);
      insertAuditLog(db, { eventType: "account_role_review_required", entityType: "account_role", entityId: role, message: `Imported chart requires explicit review of ${role}; native seed ${currentRow!.account_no} was deactivated`, createdBy: "system", createdByProgram: "rentemester-import" });
      reviewRequired.add(role);
    }
    for (const proposal of valid) {
      if (currentRow?.confirmation_source === "native_seed" && !differsFromNative && proposal.accountNo === currentRow.account_no) {
        ignored += 1;
        continue;
      }
      if (proposeAccountRole(db, role, proposal.accountNo, proposal.source)) stored += 1;
    }
  }
  return { stored, ignored, reviewRequired: [...reviewRequired] };
}

export function confirmAccountRole(db: Database, role: AccountRole, accountNo: string, actor: string,
  createdByProgram = "rentemester",
  confirmationSource: AccountRoleConfirmationSource = createdByProgram === "native-account-role-seed" ? "native_seed" : "explicit",
  dineroExport?: string) {
  if (confirmationSource === "explicit" && !/^(user|agent|system):\S.+$/.test(actor)) {
    return { ok: false as const, error: "explicit account-role confirmation requires a canonical actor (user:..., agent:..., or system:...)" };
  }
  let review: DineroVatRoleReview | undefined;
  if (dineroExport !== undefined) {
    try {
      review = readDineroVatRoleReview(dineroExport, role, accountNo);
    } catch (error) {
      return { ok: false as const, error: error instanceof Error ? error.message : String(error) };
    }
  }
  return db.transaction(() => {
    const previous = resolveAccountRole(db, role);
    if (review) {
      if (confirmationSource !== "explicit" || !matchesReviewedLiability(db, role, accountNo, review)) {
        return { ok: false as const, error: "Dinero VAT review does not match the ledger company and active legacy liability control" };
      }
    }
    if (expectations[role].type === "vat" && previous.ok && previous.accountNo === accountNo && previous.confirmationSource === confirmationSource) {
      if (review && previous.dineroReview?.inventorySha256 !== review.inventorySha256) {
        return { ok: false as const, error: "This role is already confirmed against a different Dinero source. Inspect accounts roles-status for its recorded source digest." };
      }
      return { ok: true as const, resolution: previous };
    }
    if (!review) {
      const compatibility = accountRoleCompatibility(db, role, accountNo);
      if (!compatibility.ok) return { ok: false as const, error: compatibility.error };
    }
    const current = db.query("SELECT COALESCE(MAX(version), 0) AS n FROM account_role_mappings WHERE role = ?").get(role) as { n: number };
    const version = current.n + 1;
    const previousDedicatedAccount = review ? dedicatedReverseChargeAccount(db) : null;
    runSql(db, "UPDATE account_role_mappings SET status = 'superseded' WHERE role = ? AND status = 'confirmed'", role);
    const mapping = runSql(db, "INSERT INTO account_role_mappings (role, account_no, version, confirmed_by, confirmation_source) VALUES (?, ?, ?, ?, ?)", role, accountNo, version, actor, confirmationSource);
    if (review) {
      runSql(db, `INSERT INTO dinero_vat_role_reviews
        (mapping_id, company_cvr, chart_path, chart_sha256, inventory_sha256, source_account_name, previous_dedicated_reverse_charge_account, last_existing_entry_id)
        SELECT ?, ?, ?, ?, ?, ?, ?, coalesce(max(id), 0) FROM journal_entries`,
        mapping.lastInsertRowid, review.companyCvr, review.chartPath, review.chartSha256, review.inventorySha256, review.accountName, previousDedicatedAccount);
    }
    runSql(db, "UPDATE account_role_proposals SET status = 'accepted' WHERE role = ? AND account_no = ?", role, accountNo);
    runSql(db, "UPDATE account_role_proposals SET status = 'rejected' WHERE role = ? AND account_no <> ? AND status = 'proposed'", role, accountNo);
    const evidence = review ? ` from ${review.chartPath} (SHA-256 ${review.chartSha256}, CVR ${review.companyCvr})` : "";
    insertAuditLog(db, { eventType: "account_role_confirmed", entityType: "account_role", entityId: role, message: `Confirmed ${role} -> ${accountNo} (version ${version})${evidence}`, createdBy: actor, createdByProgram });
    return { ok: true as const, resolution: resolveAccountRole(db, role) };
  }).immediate();
}

/** Seed only the native chart, and only if the account metadata still fits. */
export function seedNativeAccountRoles(db: Database) {
  const nativeRoles: Array<[AccountRole, string]> = [["bank", "2000"], ["debtors", "1100"], ["creditors", "7000"], ["output_vat", "1200"], ["input_vat", "4000"], ["reverse_charge_vat", "1200"], ["vat_settlement", "4500"], ["operational_default", "3000"]];
  for (const [role, accountNo] of nativeRoles) {
    const history = db.query("SELECT 1 FROM account_role_mappings WHERE role = ? LIMIT 1").get(role);
    const proposal = db.query("SELECT 1 FROM account_role_proposals WHERE role = ? LIMIT 1").get(role);
    if (!history && !proposal) confirmAccountRole(db, role, accountNo, "system", "native-account-role-seed", "native_seed");
  }
}
