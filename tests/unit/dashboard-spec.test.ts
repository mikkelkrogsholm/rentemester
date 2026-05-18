import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const SPEC_PATH = join(process.cwd(), "docs", "dashboard-spec.md");

const REQUIRED_SECTIONS = [
  "### 1. Header",
  "### 2. Status-tæller-stribe",
  "### 3. Næste deadline",
  "### 4. Åbne fakturaer-tabel",
  "### 5. Seneste aktivitet",
  "### 6. Backup-status",
  "### 7. Audit-chain-status",
  "### 8. Footer",
];

const REQUIRED_TOKEN_REFERENCES = [
  "paper",
  "ink",
  "amount",
  "mono-family",
  "success",
  "danger",
];

const REQUIRED_CORE_APIS = [
  "buildInvoiceList",
  "buildOverdueInvoiceList",
  "listBankTransactions",
  "listExceptions",
  "buildVatReport",
  "getBackupComplianceStatus",
  "verifyAuditChain",
  "getCompanySettings",
];

describe("dashboard spec", () => {
  test("spec file exists", () => {
    expect(existsSync(SPEC_PATH)).toBe(true);
  });

  test("contains all 8 required sections", () => {
    const body = readFileSync(SPEC_PATH, "utf8");
    for (const heading of REQUIRED_SECTIONS) {
      expect(body, `missing section heading: ${heading}`).toContain(heading);
    }
  });

  test("contains at least one ASCII layout sketch", () => {
    const body = readFileSync(SPEC_PATH, "utf8");
    // Look for a fenced code block containing box-drawing characters.
    const fences = body.match(/```[\s\S]*?```/g) ?? [];
    const hasBoxDrawing = fences.some((block) => /[│┌└├─╔╚╗╝║═]/.test(block));
    expect(hasBoxDrawing).toBe(true);
  });

  test("references at least three DESIGN.md tokens", () => {
    const body = readFileSync(SPEC_PATH, "utf8");
    const found = REQUIRED_TOKEN_REFERENCES.filter((token) =>
      body.includes(token),
    );
    expect(found.length).toBeGreaterThanOrEqual(3);
  });

  test("identifies all core API entry points needed for the dashboard", () => {
    const body = readFileSync(SPEC_PATH, "utf8");
    for (const api of REQUIRED_CORE_APIS) {
      expect(body, `missing core API reference: ${api}`).toContain(api);
    }
  });

  test("declares an on-demand refresh cadence", () => {
    const body = readFileSync(SPEC_PATH, "utf8");
    expect(body).toMatch(/on-demand/i);
    expect(body).toMatch(/rentemester dashboard/);
  });

  test("declares danish default with documented formatting", () => {
    const body = readFileSync(SPEC_PATH, "utf8");
    expect(body).toMatch(/dansk default/i);
    expect(body).toContain("1.234,56 DKK");
  });

  test("declares determinism contract for render-engine inputs", () => {
    const body = readFileSync(SPEC_PATH, "utf8");
    expect(body).toMatch(/determinisme/i);
    expect(body).toContain("asOfDate");
    expect(body).toContain("generatedAt");
  });
});
