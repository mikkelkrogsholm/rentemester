// Tests: src/core/rules-metadata.ts
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { currentRuleBundleVersion, readLegalSourceIds, readRuleBundleMetadata } from "../../src/core/rules-metadata";

describe("rule and source metadata consistency", () => {
  test("all YAML source references resolve to declared legal sources", () => {
    const legalSources = new Set(readLegalSourceIds());
    for (const bundle of readRuleBundleMetadata()) {
      for (const sourceId of [...bundle.declaredSources, ...bundle.sourceIds]) {
        expect(legalSources.has(sourceId)).toBe(true);
      }
    }
  });

  test("every rule ID referenced by code exists in rules bundles", () => {
    const yamlRuleIds = new Set(readRuleBundleMetadata().flatMap((bundle) => bundle.ruleIds));
    const codeRuleIds = new Set<string>();
    for (const file of readdirSync("src/core").filter((entry) => entry.endsWith(".ts"))) {
      const content = readFileSync(join("src/core", file), "utf8");
      for (const match of content.matchAll(/["'`](DK-[A-Z0-9-]+-\d{3})["'`]/g)) {
        codeRuleIds.add(match[1]);
      }
    }

    expect([...codeRuleIds].filter((ruleId) => !yamlRuleIds.has(ruleId))).toEqual([]);
  });

  test("every YAML rule ID is surfaced by code", () => {
    const yamlRuleIds = new Set(readRuleBundleMetadata().flatMap((bundle) => bundle.ruleIds));
    const codeText = readdirSync("src/core")
      .filter((entry) => entry.endsWith(".ts"))
      .map((entry) => readFileSync(join("src/core", entry), "utf8"))
      .join("\n");

    expect([...yamlRuleIds].filter((ruleId) => !codeText.includes(ruleId))).toEqual([]);
  });

  test("every VAT code in rules is referenced by code or seeded accounts", () => {
    const vatCodes = new Set(readRuleBundleMetadata().flatMap((bundle) => bundle.vatCodes));
    const codeText = readdirSync("src/core")
      .filter((entry) => entry.endsWith(".ts"))
      .map((entry) => readFileSync(join("src/core", entry), "utf8"))
      .join("\n");

    expect([...vatCodes].filter((vatCode) => !codeText.includes(vatCode))).toEqual([]);
  });

  test("ledger rule version reflects current YAML bundle versions", () => {
    const bundleVersion = currentRuleBundleVersion();
    expect(bundleVersion).toContain("bookkeeping=dk-bookkeeping-v0.0.6");
    expect(bundleVersion).toContain("documents=dk-documents-v0.0.3");
    expect(bundleVersion).toContain("gdpr=dk-gdpr-v0.0.1");
    expect(bundleVersion).toContain("invoices=dk-invoices-v0.0.7");
    expect(bundleVersion).toContain("peppol=dk-peppol-v0.0.2");
    expect(bundleVersion).toContain("vat=dk-vat-v0.0.3");
  });
});
