// Tests: src/core/regulatory-coverage.ts, src/core/rules-metadata.ts
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  computeRegulatoryCoverage,
  evaluateScope,
  parseScopeManifest,
  type ScopeManifest,
} from "../../src/core/regulatory-coverage";
import type { Provision } from "../../src/core/legal-provisions";
import {
  parseRuleBundle,
  readRuleMetadata,
  type RuleMetadata,
} from "../../src/core/rules-metadata";

describe("regulatory coverage", () => {
  test("no rule citation has a closure error", () => {
    // A closure error means a rule cites a statutory provision that does not
    // resolve in its declared source. The graph of rule -> law must close.
    const coverage = computeRegulatoryCoverage();
    expect(coverage.closureErrors).toEqual([]);
  });

  test("no rule citation has a drift error", () => {
    // A drift error means the cited provision's text hash no longer matches
    // the extractor — the legislation text changed since the rule was reviewed.
    const coverage = computeRegulatoryCoverage();
    expect(coverage.driftErrors).toEqual([]);
  });

  test("computeRegulatoryCoverage is deterministic", () => {
    const first = computeRegulatoryCoverage();
    const second = computeRegulatoryCoverage();
    expect(first).toEqual(second);
  });

  test("no rule citation has a scope error", () => {
    // A scope error means the scope manifest is incomplete: a missing source,
    // a bad range endpoint, or a citation outside the declared scope. The gate
    // hard-fails so an unreviewed scope gap cannot land.
    const coverage = computeRegulatoryCoverage();
    expect(coverage.scopeErrors).toEqual([]);
  });

  test("the coverage metric is internally consistent", () => {
    const coverage = computeRegulatoryCoverage();
    expect(coverage.overall.operativeCount).toBeGreaterThanOrEqual(0);
    expect(coverage.overall.citedCount).toBeGreaterThanOrEqual(0);
    expect(coverage.overall.citedCount).toBeLessThanOrEqual(coverage.overall.operativeCount);

    let summedOperative = 0;
    let summedCited = 0;
    for (const source of coverage.perSource) {
      expect(source.citedCount).toBeGreaterThanOrEqual(0);
      expect(source.citedCount).toBeLessThanOrEqual(source.operativeCount);
      expect(source.coveredRefs.length).toBe(source.citedCount);
      expect(source.uncoveredRefs.length).toBe(source.operativeCount - source.citedCount);
      summedOperative += source.operativeCount;
      summedCited += source.citedCount;
    }
    expect(summedOperative).toBe(coverage.overall.operativeCount);
    expect(summedCited).toBe(coverage.overall.citedCount);
  });

  test("the in-scope metric is internally consistent", () => {
    const coverage = computeRegulatoryCoverage();
    // The headline metric (in-scope cited / in-scope operative) must be a
    // well-formed fraction and a subset of the raw corpus-wide figures.
    expect(coverage.overall.inScopeCitedCount).toBeGreaterThanOrEqual(0);
    expect(coverage.overall.inScopeCitedCount).toBeLessThanOrEqual(
      coverage.overall.inScopeOperativeCount,
    );
    expect(coverage.overall.inScopeOperativeCount).toBeLessThanOrEqual(
      coverage.overall.operativeCount,
    );
    expect(coverage.overall.inScopeCitedCount).toBeLessThanOrEqual(
      coverage.overall.citedCount,
    );

    let summedInScopeOperative = 0;
    let summedInScopeCited = 0;
    for (const source of coverage.perSource) {
      expect(source.inScopeCitedCount).toBeGreaterThanOrEqual(0);
      expect(source.inScopeCitedCount).toBeLessThanOrEqual(source.inScopeOperativeCount);
      expect(source.inScopeOperativeCount).toBeLessThanOrEqual(source.operativeCount);
      expect(source.inScopeCitedCount).toBeLessThanOrEqual(source.citedCount);
      summedInScopeOperative += source.inScopeOperativeCount;
      summedInScopeCited += source.inScopeCitedCount;
    }
    expect(summedInScopeOperative).toBe(coverage.overall.inScopeOperativeCount);
    expect(summedInScopeCited).toBe(coverage.overall.inScopeCitedCount);

    // With every cited provision in scope (the gate), in-scope cited equals
    // the raw cited count — the headline metric only narrows the denominator.
    expect(coverage.overall.inScopeCitedCount).toBe(coverage.overall.citedCount);
  });

  test("uncited rules are exactly the rules with no provisions block", () => {
    const coverage = computeRegulatoryCoverage();
    const expectedUncited = readRuleMetadata()
      .filter((rule) => rule.provisions.length === 0)
      .map((rule) => rule.ruleId)
      .sort();
    expect(coverage.uncitedRules).toEqual(expectedUncited);
  });

  test("uncited rules match the documented allowlist", () => {
    // CI gate: any rule landing without a `provisions:` block must be on this
    // allowlist, with a one-line explanation of why it is intentionally
    // uncited. Two valid reasons:
    //   1. Workflow-guardrail rule that is not keyed to a single statutory
    //      provision (see the rule's own YAML comment).
    //   2. Source not yet ingestable into the corpus — either the URL on
    //      retsinformation.dk is wrong/historic (NemHandel, PEPPOL public
    //      payments), or the source is an EU regulation that the paragraf-
    //      aware extractor does not model (GDPR uses `art.` not `§`).
    // Removing a rule from this list requires either citing it or moving
    // the source into `sources/downloaded/` and `sources/scope.yaml`.
    const allowedUncited = [
      // Workflow-guardrail (no single statutory hook):
      "DK-ANNUAL-REPORT-CLASS-B-001",
      "DK-ANNUAL-REPORT-IXBRL-002",
      "DK-TAX-RETURN-CORP-001",
      // Deliberately out of declared legal scope:
      "DK-VAT-OSS-001",
      // Source not yet ingested (DK-OFFENTLIGE-BETALINGER-2007-798 is
      // historic; needs the current LBK):
      "DK-INVOICE-PUBLIC-EXPORT-001",
      "DK-INVOICE-PUBLIC-OIOUBL-001",
      "DK-INVOICE-PUBLIC-RECIPIENT-001",
      "DK-PEPPOL-SUBMIT-001",
      // EU regulation; paragraf-aware extractor does not model `art.`:
      "GDPR-RETENTION-BOUNDED-ERASURE",
      "GDPR-SUBJECT-EXPORT",
    ].sort();
    const coverage = computeRegulatoryCoverage();
    expect(coverage.uncitedRules).toEqual(allowedUncited);
  });

  test("the reverse map carries code and test files for every cited rule", () => {
    const coverage = computeRegulatoryCoverage();
    expect(coverage.reverseMap.length).toBeGreaterThan(0);
    let withCode = 0;
    for (const record of coverage.reverseMap) {
      for (const entry of record.rules) {
        if (entry.codeFiles.length > 0) withCode += 1;
      }
    }
    expect(withCode).toBeGreaterThan(0);
  });

  test("the reverse map only references cited provisions and known rules", () => {
    const coverage = computeRegulatoryCoverage();
    const ruleIds = new Set(readRuleMetadata().map((rule) => rule.ruleId));
    const citedKeys = new Set<string>();
    for (const source of coverage.perSource) {
      for (const ref of source.coveredRefs) citedKeys.add(`${source.sourceId} ${ref}`);
    }
    for (const record of coverage.reverseMap) {
      expect(citedKeys.has(`${record.sourceId} ${record.ref}`)).toBe(true);
      for (const entry of record.rules) {
        expect(ruleIds.has(entry.ruleId)).toBe(true);
        expect(Array.isArray(entry.codeFiles)).toBe(true);
        expect(Array.isArray(entry.testFiles)).toBe(true);
      }
    }
  });
});

describe("rule citation parser rejects malformed input", () => {
  const ruleHead = [
    "rules:",
    "  - rule_id: DK-TEST-001",
    "    name: test rule",
    "    category: test",
    "    source_id: DK-RENTELOVEN-2014-459",
  ];

  test("a citation with a ref but no text_hash throws instead of dropping it", () => {
    const yaml = [
      ...ruleHead,
      "    provisions:",
      '      - ref: "§ 3, stk. 1"',
      "    severity: hard_stop",
    ].join("\n");
    expect(() => parseRuleBundle(yaml, "invoices")).toThrow(/ref and text_hash/);
  });

  test("a provisions entry that is not `- ref: ...` throws", () => {
    const yaml = [
      ...ruleHead,
      "    provisions:",
      '      - text_hash: "sha256:abc"',
      "    severity: hard_stop",
    ].join("\n");
    expect(() => parseRuleBundle(yaml, "invoices")).toThrow(/must be/);
  });

  test("tab indentation throws", () => {
    const yaml = [...ruleHead, "\tseverity: hard_stop"].join("\n");
    expect(() => parseRuleBundle(yaml, "invoices")).toThrow(/tab indentation/);
  });

  test("a duplicated text_hash within one entry throws", () => {
    const yaml = [
      ...ruleHead,
      "    provisions:",
      '      - ref: "§ 3, stk. 1"',
      '        text_hash: "sha256:abc"',
      '        text_hash: "sha256:def"',
      "    severity: hard_stop",
    ].join("\n");
    expect(() => parseRuleBundle(yaml, "invoices")).toThrow(/duplicate text_hash/);
  });

  test("a well-formed citation parses to a ref/text_hash pair", () => {
    const yaml = [
      ...ruleHead,
      "    provisions:",
      '      - ref: "§ 3, stk. 1"',
      '        text_hash: "sha256:abc"',
      "    severity: hard_stop",
    ].join("\n");
    const rules = parseRuleBundle(yaml, "invoices");
    expect(rules).toHaveLength(1);
    expect(rules[0].provisions).toEqual([{ ref: "§ 3, stk. 1", textHash: "sha256:abc" }]);
  });

  test("a folded block-scalar explanation is joined into one line", () => {
    const yaml = [
      ...ruleHead,
      "    severity: hard_stop",
      "    explanation: >-",
      "      First line of the explanation",
      "      continues onto a second line.",
    ].join("\n");
    const rules = parseRuleBundle(yaml, "invoices");
    expect(rules[0].explanation).toBe(
      "First line of the explanation continues onto a second line.",
    );
  });

  test("a literal block scalar throws rather than folding newlines away", () => {
    const yaml = [
      ...ruleHead,
      "    severity: hard_stop",
      "    explanation: |",
      "      line one",
      "      line two",
    ].join("\n");
    expect(() => parseRuleBundle(yaml, "invoices")).toThrow(/literal block scalars/);
  });

  test("a multi-line plain scalar throws instead of dropping continuation lines", () => {
    const yaml = [
      ...ruleHead,
      "    severity: hard_stop",
      "    explanation: first line of the explanation",
      "      a dropped continuation line",
    ].join("\n");
    expect(() => parseRuleBundle(yaml, "invoices")).toThrow(/unexpected indented line/);
  });

  test("the machine_rule subtree is skipped, not mistaken for stray lines", () => {
    const yaml = [
      ...ruleHead,
      "    severity: hard_stop",
      "    machine_rule:",
      "      require:",
      "        - some_condition",
      "        - another_condition",
      "    explanation: after the machine rule",
    ].join("\n");
    const rules = parseRuleBundle(yaml, "invoices");
    expect(rules).toHaveLength(1);
    expect(rules[0].explanation).toBe("after the machine rule");
  });
});

describe("scope manifest parser rejects malformed input", () => {
  const goodManifest = [
    "version: dk-scope-test-v1",
    "sources:",
    "  DK-RENTELOVEN-2014-459:",
    "    in_scope:",
    '      - "§ 1-§ 9b"',
    "  DK-BILAG-OPBEVARING-2023-1383:",
    "    in_scope: all",
  ].join("\n");

  test("a well-formed manifest parses into a version and a sources map", () => {
    const manifest = parseScopeManifest(goodManifest);
    expect(manifest.version).toBe("dk-scope-test-v1");
    expect(manifest.sources.get("DK-BILAG-OPBEVARING-2023-1383")).toEqual({ kind: "all" });
    expect(manifest.sources.get("DK-RENTELOVEN-2014-459")).toEqual({
      kind: "ranges",
      ranges: [{ low: "1", high: "9b" }],
    });
  });

  test("the real sources/scope.yaml parses without throwing", () => {
    const text = readFileSync(
      join(import.meta.dir, "../../sources/scope.yaml"),
      "utf8",
    );
    const manifest = parseScopeManifest(text);
    expect(manifest.version.length).toBeGreaterThan(0);
    expect(manifest.sources.size).toBeGreaterThan(0);
  });

  test("a missing version throws", () => {
    const yaml = ["sources:", "  DK-X:", "    in_scope: all"].join("\n");
    expect(() => parseScopeManifest(yaml)).toThrow(/missing version/);
  });

  test("a source with no in_scope declaration throws", () => {
    const yaml = ["version: v1", "sources:", "  DK-X:"].join("\n");
    expect(() => parseScopeManifest(yaml)).toThrow(/no in_scope/);
  });

  test("an inverted range throws", () => {
    const yaml = [
      "version: v1",
      "sources:",
      "  DK-X:",
      "    in_scope:",
      '      - "§ 9-§ 3"',
    ].join("\n");
    expect(() => parseScopeManifest(yaml)).toThrow(/inverted/);
  });

  test("a range token that is not `§ ...` throws", () => {
    const yaml = [
      "version: v1",
      "sources:",
      "  DK-X:",
      "    in_scope:",
      '      - "1-9"',
    ].join("\n");
    expect(() => parseScopeManifest(yaml)).toThrow(/bad range/);
  });

  test("tab indentation throws", () => {
    const yaml = ["version: v1", "sources:", "\tDK-X:"].join("\n");
    expect(() => parseScopeManifest(yaml)).toThrow(/tab indentation/);
  });
});

describe("the three scope hard-error checks", () => {
  const provision = (sourceId: string, paragraf: string): Provision => ({
    sourceId,
    ref: `§ ${paragraf}, stk. 1`,
    path: [paragraf, "1"],
    kind: "operative",
    text: "stub text",
    textHash: "sha256:stub",
  });

  const provisions = new Map<string, Provision[]>([
    ["DK-A", [provision("DK-A", "1"), provision("DK-A", "2"), provision("DK-A", "3")]],
    ["DK-B", [provision("DK-B", "5")]],
  ]);

  const rule = (sourceId: string, ref: string): RuleMetadata => ({
    ruleId: "DK-TEST-001",
    sourceId,
    bundle: "test",
    name: "test rule",
    explanation: "test",
    provisions: [{ ref, textHash: "sha256:stub" }],
    severity: "hard_stop",
    category: "test",
  });

  const manifest = (text: string): ScopeManifest => parseScopeManifest(text);

  test("(a) a downloaded source missing from the manifest is a scope error", () => {
    // DK-B is downloaded but absent from the manifest.
    const scope = manifest(
      ["version: v1", "sources:", "  DK-A:", "    in_scope: all"].join("\n"),
    );
    const errors = evaluateScope(provisions, scope, []);
    expect(errors).toEqual([{ kind: "missing_source", sourceId: "DK-B" }]);
  });

  test("(b) a range endpoint that is not a paragraf in the source is a scope error", () => {
    // DK-A has §§ 1-3; § 9 does not exist.
    const scope = manifest(
      [
        "version: v1",
        "sources:",
        "  DK-A:",
        "    in_scope:",
        '      - "§ 1-§ 9"',
        "  DK-B:",
        "    in_scope: all",
      ].join("\n"),
    );
    const errors = evaluateScope(provisions, scope, []);
    expect(errors).toEqual([{ kind: "bad_endpoint", sourceId: "DK-A", paragraf: "9" }]);
  });

  test("(c) a citation outside the declared scope is a scope error", () => {
    // DK-A's scope is § 1 only; the rule cites § 3.
    const scope = manifest(
      [
        "version: v1",
        "sources:",
        "  DK-A:",
        "    in_scope:",
        '      - "§ 1"',
        "  DK-B:",
        "    in_scope: all",
      ].join("\n"),
    );
    const errors = evaluateScope(provisions, scope, [rule("DK-A", "§ 3, stk. 1")]);
    expect(errors).toEqual([
      {
        kind: "citation_out_of_scope",
        ruleId: "DK-TEST-001",
        sourceId: "DK-A",
        ref: "§ 3, stk. 1",
      },
    ]);
  });

  test("a fully covered manifest produces no scope errors", () => {
    const scope = manifest(
      [
        "version: v1",
        "sources:",
        "  DK-A:",
        "    in_scope:",
        '      - "§ 1-§ 3"',
        "  DK-B:",
        "    in_scope: all",
      ].join("\n"),
    );
    const errors = evaluateScope(provisions, scope, [
      rule("DK-A", "§ 3, stk. 1"),
      rule("DK-B", "§ 5, stk. 1"),
    ]);
    expect(errors).toEqual([]);
  });
});
