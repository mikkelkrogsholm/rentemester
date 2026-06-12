// System-level read handlers: health, CVR-login status, lovgrundlag (rules).

import type { ServerConfig } from "../config";
import {
  readLegalSources,
  readRuleBundleMetadata,
  readRuleMetadata,
} from "../../core/rules-metadata";
import { okResponse } from "./_shared";

export function handleHealth(
  config: ServerConfig,
  routes: ReadonlyArray<{ method: string; pattern: string; summary: string }>,
): Response {
  return okResponse({
    service: "rentemester-cockpit",
    workspace: config.workspaceRoot,
    authRequired: config.authRequired,
    routes,
  });
}

/**
 * #402 — tells the cockpit whether the CVR-register login (`CVR_USERNAME`
 * / `CVR_PASSWORD`) is configured on the server. The cockpit reads this
 * *before* it shows the "Hent fra CVR" button so the owner sees a friendly
 * "log ind med dit virk.dk-login" message instead of clicking a button that
 * fails silently or returns a raw API error.
 *
 * The endpoint deliberately returns only a boolean — never the credential
 * values themselves — so it stays safe to call from the browser.
 */
export function handleSystemCvrStatus(): Response {
  const configured = Boolean(process.env.CVR_USERNAME && process.env.CVR_PASSWORD);
  return okResponse({ cvrStatus: { configured } });
}

/**
 * GET /api/rules — Lovgrundlag-viewer (#347).
 *
 * Eksponerer rules/dk-bundlerne + tilhørende retsinformation-citationer så
 * cockpittet kan vise SMB-ejeren *hvilke regler* der styrer bogføringen og
 * *hvor* de er hentet fra. Read-only: regler kan kun ændres via PR i
 * `rules/dk/`. Bundler, regler og legal-sources hentes via de eksisterende
 * helpers (`readRuleBundleMetadata`, `readRuleMetadata`, `readLegalSources`)
 * så der ikke er duplikeret parsing.
 */
export function handleRules(): Response {
  const bundles = readRuleBundleMetadata().map((b) => ({
    name: b.name,
    version: b.version,
    ruleCount: b.ruleIds.length,
    sources: b.declaredSources,
    vatCodes: b.vatCodes,
  }));
  const rules = readRuleMetadata().map((r) => ({
    ruleId: r.ruleId,
    bundle: r.bundle,
    sourceId: r.sourceId,
    name: r.name,
    explanation: r.explanation,
    severity: r.severity,
    category: r.category,
    provisions: r.provisions,
  }));
  const sources = readLegalSources();
  return okResponse({ ruleBundles: bundles, rules, legalSources: sources });
}
