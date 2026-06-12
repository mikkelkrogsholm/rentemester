// Tests: src/core/compliance-report.ts
import { describe, expect, test } from "bun:test";
import {
  renderComplianceReport,
  complianceReportFingerprint,
  type ComplianceReportInput,
} from "../../src/core/compliance-report";
import type { BackupGovernanceStatus } from "../../src/core/backup-governance";
import type { RetentionStatusReport } from "../../src/core/retention";
import type { RegulatoryCoverage } from "../../src/core/regulatory-coverage";

function baseInput(): ComplianceReportInput {
  const backup: BackupGovernanceStatus = {
    ok: true,
    appliedRules: ["DK-BOOKKEEPING-BACKUP-001"],
    compliance: {
      ok: true,
      appliedRules: ["DK-BOOKKEEPING-BACKUP-001"],
      backupDue: false,
      latestBackupId: "backup-20260517T020900Z",
      latestBackupAt: "2026-05-17T02:09:00.000Z",
      nextBackupDeadline: "2026-05-24T02:09:00.000Z",
      retentionDeadline: null,
      errors: [],
    },
    lock: { mode: "voluntary", enforced: false, locked: false, reason: null, errors: [] },
    destinations: [
      {
        id: "dest-1",
        label: "EU Dropbox",
        kind: "dropbox",
        location: "/mnt/eu/dropbox",
        nonRelatedParty: true,
        regionAttestation: {
          inEeaOrEu: true,
          country: "DK",
          attestedBy: "user:demo",
          attestedAt: "2026-05-17T02:09:30.000Z",
          note: null,
        },
        itSecurityAttestation: {
          meetsRecognisedStandards: true,
          attestedBy: "user:demo",
          attestedAt: "2026-05-17T02:09:30.000Z",
          note: null,
        },
        createdAt: "2026-05-17T02:09:30.000Z",
        createdBy: "user:demo",
        placements: [],
      },
    ],
    destinationCount: 1,
    compliantDestinationCount: 1,
    hasCompliantDestination: true,
    latestBackupPlacedOffsite: true,
    latestBackupPlacementCount: 1,
    checkedAt: "2026-05-17T02:30:00.000Z",
    errors: [],
  };
  const retention: RetentionStatusReport = {
    ok: true,
    asOf: "2026-05-17",
    appliedRules: ["DK-BOOKKEEPING-RETENTION-001"],
    rows: [
      { table: "documents", total: 7, expired: 0, nextExpiry: "2031-12-31", oldestExpired: null },
      { table: "journal_entries", total: 15, expired: 0, nextExpiry: "2031-12-31", oldestExpired: null },
      { table: "bank_transactions", total: 6, expired: 0, nextExpiry: "2031-12-31", oldestExpired: null },
    ],
    errors: [],
  };
  const coverage: RegulatoryCoverage = {
    overall: {
      operativeCount: 100,
      citedCount: 30,
      inScopeOperativeCount: 50,
      inScopeCitedCount: 30,
    },
    perSource: [
      {
        sourceId: "DK-BOGFORINGSLOVEN-2022-700",
        operativeCount: 60,
        citedCount: 8,
        inScopeOperativeCount: 30,
        inScopeCitedCount: 8,
        coveredRefs: [],
        uncoveredRefs: [],
      },
    ],
    closureErrors: [],
    driftErrors: [],
    scopeErrors: [],
    uncitedRules: [],
    uncoveredProvisions: [],
    reverseMap: [],
  };
  return {
    generatedAt: "2026-05-17T02:30:00.000Z",
    companyName: "Demo ApS",
    companyCvr: "12345678",
    fiscalYearLabel: "2026",
    commitSha: "abcdef0",
    ruleBundleVersion: "bookkeeping=dk-bookkeeping-v0.0.7",
    audit: { ok: true, entryCount: 15, errors: [] },
    backup,
    retention,
    periods: { closedCount: 0, lastClosedLabel: null },
    gdpr: { eventCount: 0, fingerprint: "sha256:abc123" },
    coverage,
    rules: [
      {
        ruleId: "DK-BOOKKEEPING-BALANCED-001",
        sourceId: "DK-BOGFORINGSLOVEN-2022-700",
        bundle: "bookkeeping",
        name: "Double-entry postings must balance",
        explanation: "...",
        provisions: [{ ref: "§ 7, stk. 1", textHash: "sha256:abc" }],
        severity: "hard_stop",
        category: "ledger_validation",
      },
    ],
  };
}

describe("renderComplianceReport", () => {
  test("is byte-for-byte deterministic for identical input", () => {
    const a = renderComplianceReport(baseInput());
    const b = renderComplianceReport(baseInput());
    expect(a).toBe(b);
    expect(complianceReportFingerprint(a)).toBe(complianceReportFingerprint(b));
  });

  test("contains all nine section headings + the business overview", () => {
    const html = renderComplianceReport(baseInput());
    expect(html).toContain("Forretningsmæssigt overblik");
    expect(html).toContain("1. Integritet af bogføringen");
    expect(html).toContain("2. Opbevaring og backup");
    expect(html).toContain("3. Opbevaringsfrist (5 år)");
    expect(html).toContain("4. Periode-lukning");
    expect(html).toContain("5. GDPR — persondata");
    expect(html).toContain("6. Regulatorisk dækning");
    expect(html).toContain("7. Regler og deres lovhjemmel");
    expect(html).toContain("8. Myndighedsudlevering og SAF-T");
  });

  test("escapes user-controlled fields (company name + cvr)", () => {
    const input = baseInput();
    input.companyName = "<script>alert(1)</script>";
    input.companyCvr = "12<34&56\"78";
    const html = renderComplianceReport(input);
    expect(html).not.toContain("<script>alert(1)</script>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).toContain("12&lt;34&amp;56&quot;78");
  });

  test("shows broken-chain pill when audit verify failed", () => {
    const input = baseInput();
    input.audit = { ok: false, entryCount: 3, errors: ["entry 2 hash mismatch"] };
    const html = renderComplianceReport(input);
    expect(html).toContain("Brudt kæde");
    expect(html).toContain("entry 2 hash mismatch");
  });

  test("shows the uncited count and the per-source coverage breakdown", () => {
    const input = baseInput();
    input.coverage.uncitedRules = ["DK-X-001", "DK-Y-002"];
    const html = renderComplianceReport(input);
    expect(html).toContain("Uncited regler");
    expect(html).toContain(">2<"); // the count cell
    expect(html).toContain("DK-BOGFORINGSLOVEN-2022-700");
    expect(html).toContain("8/30");
  });

  test("the fingerprint changes when ledger state changes", () => {
    const a = renderComplianceReport(baseInput());
    const input = baseInput();
    input.audit.entryCount = 99;
    const b = renderComplianceReport(input);
    expect(a).not.toBe(b);
  });

  test("renders uncited rules as 'ingen citation' inline in the rule table", () => {
    const input = baseInput();
    input.rules = [
      {
        ruleId: "DK-FOO",
        sourceId: "DK-BOGFORINGSLOVEN-2022-700",
        bundle: "bookkeeping",
        name: "foo",
        explanation: "...",
        provisions: [],
        severity: "advisory",
        category: "workflow_guardrail",
      },
    ];
    const html = renderComplianceReport(input);
    expect(html).toContain("ingen citation");
  });
});
