import type { Page } from "@playwright/test";

export type ConsolidationSourceFixture = {
  profile: Record<string, unknown> & { id: string; groupId: string };
  from: string;
  asOf: string;
  companySlugs: string[];
  reportAssertions: Array<{ path: string[]; value: unknown }>;
};

/** Explicit independent expectations for the two-orientation elimination source fixture. */
export function eliminationConsolidationFixture(anchor: string, other: string, asOf: string): ConsolidationSourceFixture {
  if (anchor === other || ![anchor, other].every(s => /^synthetic-eliminations-[a-z0-9-]+$/.test(s)) || !/^\d{4}-\d{2}-\d{2}$/.test(asOf)) {
    throw Error("Dedicated completed elimination source fixture required");
  }
  const reportingLines = [["assets", "Assets", "asset"], ["debt", "Debt", "liability"],
    ["equity", "Equity", "equity"], ["result", "Current result", "equity"],
    ["income", "Income", "income"], ["expense", "Expense", "expense"]]
    .map(([id, label, section], displayOrder) => ({ id, label, section, displayOrder, ...(id === "result" ? { role: "current-result" } : {}) }));
  const mappings = [
    [anchor, "1861", "assets"], [anchor, "2862", "debt"], [anchor, "1869", "assets"], [anchor, "8861", "equity"],
    [other, "2861", "debt"], [other, "1862", "assets"], [other, "8861", "equity"],
  ];
  return {
    profile: { version: 1, id: "synthetic-eliminations-report-profile", groupId: "synthetic-eliminations", currency: "DKK",
      reportingLines, accountMappings: mappings.map(([companySlug, accountNo, reportingLineId], i) => ({
        id: `elimination-report-${i}`, companySlug, accountNo, reportingLineId, validFrom: "2000-01-01",
      })), evidenceRefs: ["synthetic-reviewed-two-orientation-report"], validFrom: "2000-01-01" },
    from: asOf, asOf, companySlugs: [anchor, other], reportAssertions: [
      { path: ["status"], value: "ready" },
      { path: ["consolidatedFigures", "0", "rawCompanySum"], value: "70.12" },
      { path: ["consolidatedFigures", "0", "eliminationAdjustment"], value: "-69.12" },
      { path: ["consolidatedFigures", "0", "consolidatedAmount"], value: "1.00" },
      { path: ["consolidatedFigures", "1", "rawCompanySum"], value: "69.12" },
      { path: ["consolidatedFigures", "1", "eliminationAdjustment"], value: "-69.12" },
      { path: ["consolidatedFigures", "1", "consolidatedAmount"], value: "0.00" },
      { path: ["consolidatedFigures", "2", "consolidatedAmount"], value: "1.00" },
      ...[3, 4, 5].map(i => ({ path: ["consolidatedFigures", String(i), "consolidatedAmount"], value: "0.00" })),
      { path: ["appliedEliminations", "length"], value: 2 },
    ],
  };
}

/** Prepare real source journals through ordinary APIs in a fresh dedicated workspace. */
export async function prepareConsolidationClients(
  author: Page, base: string, anchor: string, other: string,
): Promise<ConsolidationSourceFixture> {
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base) || anchor === other ||
      ![anchor, other].every(s => /^synthetic-consolidation-clients-[a-z0-9-]+$/.test(s))) {
    throw Error("Explicit owned synthetic consolidation pair required");
  }
  const headers = { origin: base };
  async function get(path: string) {
    const r = await author.request.get(base + path);
    if (!r.ok()) throw Error(`Consolidation source read refused ${r.status()}`);
    return r.json();
  }
  async function post(path: string, data: unknown) {
    const r = await author.request.post(base + path, { headers, data });
    if (!r.ok()) throw Error(`Ordinary consolidation source command refused ${r.status()}`);
    return r.json();
  }
  const current = await get(`/api/companies/${anchor}/intercompany`);
  if (current.manifest !== null || current.manifestHash !== null) throw Error("Fresh global manifest fixture required");
  const asOf = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
  for (const slug of [anchor, other]) {
    const { journal } = await get(`/api/companies/${slug}/journal`);
    if (journal.entries.length) throw Error("Fresh source ledger required");
    for (const [accountNo, type, normalBalance] of [
      ["1861", "asset", "debit"], ["3861", "income", "credit"],
    ]) await post(`/api/companies/${slug}/accounts`, { accountNo, name: `Synthetic consolidation ${accountNo}`, type, normalBalance });
    const original = await post(`/api/companies/${slug}/documents/ingest`, {
      confirm: true, fileName: `synthetic-consolidation-client-${slug}.txt`,
      fileBase64: Buffer.from(`Synthetic retained consolidation source ${slug}: DKK12.34, no VAT.\n`).toString("base64"),
      metadata: { source: "Synthetic consolidation client fixture", documentType: "cash_register_receipt" },
    });
    await post(`/api/companies/${slug}/manual-journal`, { confirm: true, input: {
      format: "rentemester-manual-journal-post/1", expectedRuleVersion: journal.nativePosting.ruleVersion,
      idempotencyKey: `synthetic-consolidation-client-${slug}`, request: {
        documentId: String(original.document.id), transactionDate: asOf, text: "Synthetic exact consolidation source", currency: "DKK",
        lines: [{ accountNo: "1861", debitAmount: "12.34", creditAmount: "0.00" },
          { accountNo: "3861", debitAmount: "0.00", creditAmount: "12.34" }],
      },
    } });
  }
  const groupId = "synthetic-consolidation-clients";
  await post(`/api/companies/${anchor}/intercompany/manifest`, { version: 1, confirm: true, groups: [{
    id: groupId, name: "Synthetic consolidation client group",
    memberships: [{ id: "client-parent", companySlug: anchor, validFrom: "2000-01-01" },
      { id: "client-child", companySlug: other, validFrom: "2000-01-01" }],
    ownership: [{ id: "client-full-child", parentCompanySlug: anchor, childCompanySlug: other,
      basisPoints: 10000, evidenceRefs: ["synthetic-reviewed-group-source"], validFrom: "2000-01-01" }],
  }] });
  const reportingLines = [["assets", "Assets", "asset"], ["debt", "Debt", "liability"],
    ["equity", "Equity", "equity"], ["result", "Current result", "equity"],
    ["income", "Income", "income"], ["expense", "Expense", "expense"]]
    .map(([id, label, section], displayOrder) => ({ id, label, section, displayOrder, ...(id === "result" ? { role: "current-result" } : {}) }));
  return {
    profile: { version: 1, id: "synthetic-consolidation-clients-profile", groupId, currency: "DKK", reportingLines,
      accountMappings: [anchor, other].flatMap((companySlug, index) => [["1861", "assets"], ["3861", "income"]]
        .map(([accountNo, reportingLineId]) => ({ id: `client-${index}-${accountNo}`, companySlug, accountNo, reportingLineId, validFrom: "2000-01-01" }))),
      evidenceRefs: ["synthetic-reviewed-reporting-chart"], validFrom: "2000-01-01" },
    from: asOf, asOf, companySlugs: [anchor, other], reportAssertions: [
      { path: ["status"], value: "ready" },
      ...[0, 3, 4].map(i => ({ path: ["consolidatedFigures", String(i), "consolidatedAmount"], value: "24.68" })),
      ...[1, 2, 5].map(i => ({ path: ["consolidatedFigures", String(i), "consolidatedAmount"], value: "0.00" })),
      { path: ["appliedEliminations"], value: [] },
    ],
  };
}
