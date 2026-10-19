import type { Page } from "@playwright/test";
import type { IntercompanyClientFixture } from "./check-native-intercompany-clients";

/** Ordinary native commands create reciprocal source ledgers in a fresh owned fixture. */
export async function prepareIntercompanyClients(
  author: Page, base: string, anchor: string, other: string,
): Promise<IntercompanyClientFixture> {
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base) || anchor === other ||
      ![anchor, other].every(slug => /^synthetic-intercompany-[a-z0-9-]+$/.test(slug))) {
    throw Error("Explicit owned synthetic intercompany source pair required");
  }
  const asOf = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
  for (const slug of [anchor, other]) {
    const account = slug === anchor ? "1851" : "2851";
    for (const [accountNo, type, normalBalance] of [
      [account, slug === anchor ? "asset" : "liability", slug === anchor ? "debit" : "credit"],
      ["8851", "equity", "credit"],
    ]) {
      const response = await author.request.post(`${base}/api/companies/${slug}/accounts`, {
        headers: { origin: base }, data: { accountNo, name: `Synthetic intercompany ${accountNo}`, type, normalBalance },
      });
      if (!response.ok()) throw Error(`Ordinary synthetic source account refused ${response.status()}`);
    }
    const read = await author.request.get(`${base}/api/companies/${slug}/journal`);
    if (!read.ok()) throw Error(`Synthetic source ledger read refused ${read.status()}`);
    const { journal } = await read.json();
    const left = slug === anchor;
    const posted = await author.request.post(`${base}/api/companies/${slug}/manual-journal`, {
      headers: { origin: base }, data: { confirm: true, input: {
        format: "rentemester-manual-journal-post/1", expectedRuleVersion: journal.nativePosting.ruleVersion,
        idempotencyKey: `synthetic-intercompany-client-${slug}`, request: {
          transactionDate: asOf, text: "Synthetic client reciprocal source", currency: "DKK",
          lines: [
            { accountNo: account, debitAmount: left ? "12.34" : "0.00", creditAmount: left ? "0.00" : "12.34" },
            { accountNo: "8851", debitAmount: left ? "0.00" : "12.34", creditAmount: left ? "12.34" : "0.00" },
          ],
        },
      } },
    });
    if (!posted.ok()) throw Error(`Ordinary synthetic reciprocal source posting refused ${posted.status()}`);
  }
  return {
    manifest: { version: 1, groups: [{ id: "synthetic-intercompany", name: "Synthetic reciprocal client group",
      memberships: [{ id: "left-member", companySlug: anchor, validFrom: "2000-01-01" },
        { id: "right-member", companySlug: other, validFrom: "2000-01-01" }], ownership: [] }] },
    mapping: { id: "synthetic-client-reciprocal", groupId: "synthetic-intercompany", leftCompanySlug: anchor,
      rightCompanySlug: other, leftAccountNos: ["1851"], rightAccountNos: ["2851"],
      leftPosition: "receivable", rightPosition: "payable", evidenceRefs: ["synthetic-reviewed-client-reciprocal"], validFrom: "2000-01-01" },
    asOf, expectedBalance: "12.34",
  };
}
