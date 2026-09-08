import { openCurrentLedgerReadOnly } from "../../core/ledger-inspection";
import { evaluatePostingRules } from "../../core/posting-rules";
import { requireCompanyDbPath } from "../data/shared";
import type { ServerConfig } from "../config";
import { okResponse } from "./_shared";

/** Read-only cockpit data is always opened from the route's already-authorized slug. */
export function handleCompanyPostingRules(config: ServerConfig, slug: string): Response {
  const db = openCurrentLedgerReadOnly(requireCompanyDbPath(config.workspaceRoot, slug));
  try {
    const companyId = (db.query("SELECT id FROM companies ORDER BY id LIMIT 1").get() as { id: number }).id;
    const rules = db.query("SELECT rule_id AS ruleId, version, effective_from AS effectiveFrom, effective_to AS effectiveTo, conditions_json AS conditionsJson, outcome_json AS outcomeJson, provenance, rationale, created_by AS createdBy, payload_hash AS payloadHash, created_at AS createdAt FROM posting_rule_versions WHERE company_id=? ORDER BY rule_id, version").all(companyId);
    return okResponse({ postingRules: rules });
  } finally { db.close(); }
}
export function handleCompanyPostingRuleExplain(config: ServerConfig, slug: string, context: Record<string, unknown>, at?: string): Response {
  const db = openCurrentLedgerReadOnly(requireCompanyDbPath(config.workspaceRoot, slug));
  try { const company = (db.query("SELECT id FROM companies ORDER BY id LIMIT 1").get() as { id: number }).id; return okResponse({ dryRun: true, evaluation: evaluatePostingRules(db, { ...context, company }, { at }) }); } finally { db.close(); }
}
