import { expect, test, describe } from "bun:test";
import { billyParser } from "../../src/core/import/billy";
import type { MultiArtifactSource } from "../../src/core/import/types";

function makeArtifact(name: string, text: string) {
  return {
    name,
    path: `/test/${name}`,
    bytes: new TextEncoder().encode(text),
    text,
  };
}

function makeSource(files: Record<string, string>): MultiArtifactSource {
  const artifacts: MultiArtifactSource["files"] = {};
  for (const [name, text] of Object.entries(files)) {
    artifacts[name] = makeArtifact(name, text);
  }
  return { rootDir: "/test", files: artifacts };
}

describe("Billy import parser", () => {
  test("parses organisation master data", () => {
    const result = billyParser.parseSource!(
      makeSource({
        "organization.json": JSON.stringify({
          id: "org-1",
          name: "Test ApS",
          registrationNo: "DK12345678",
          street: "Testvej 1",
          zipcode: "5000",
          city: "Odense",
          countryId: "DK",
          email: "test@test.dk",
        }),
        "accounts.json": JSON.stringify([
          {
            id: "acc-1",
            accountNo: 1000,
            name: "Omsætning",
            natureId: "revenue",
          },
        ]),
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.source!.companyMasterData?.name).toBe("Test ApS");
    expect(result.source!.companyMasterData?.cvr).toBe("DK12345678");
    expect(result.source!.companyMasterData?.city).toBe("Odense");
  });

  test("classifies Billy natures onto Rentemester types", () => {
    const result = billyParser.parseSource!(
      makeSource({
        "organization.json": JSON.stringify({ id: "org-1", name: "Test" }),
        "accounts.json": JSON.stringify([
          { id: "a1", accountNo: 1000, name: "Revenue", natureId: "revenue" },
          { id: "a2", accountNo: 1810, name: "Office", natureId: "expense" },
          { id: "a3", accountNo: 5710, name: "Bank", natureId: "asset" },
          { id: "a4", accountNo: 7310, name: "Creditors", natureId: "liability" },
          { id: "a5", accountNo: 7130, name: "Equity", natureId: "equity" },
        ]),
      }),
    );

    expect(result.ok).toBe(true);
    const chart = result.source!.chartOfAccounts;
    expect(chart.length).toBe(5);

    const revenue = chart.find((a) => a.accountNo === "1000")!;
    expect(revenue.normalizedType).toBe("income");
    expect(revenue.normalBalance).toBe("credit");

    const expense = chart.find((a) => a.accountNo === "1810")!;
    expect(expense.normalizedType).toBe("expense");
    expect(expense.normalBalance).toBe("debit");

    const asset = chart.find((a) => a.accountNo === "5710")!;
    expect(asset.normalizedType).toBe("asset");
    expect(asset.normalBalance).toBe("debit");

    const liability = chart.find((a) => a.accountNo === "7310")!;
    expect(liability.normalizedType).toBe("liability");
    expect(liability.normalBalance).toBe("credit");

    const equity = chart.find((a) => a.accountNo === "7130")!;
    expect(equity.normalizedType).toBe("equity");
    expect(equity.normalBalance).toBe("credit");
  });

  test("maps tax rates to VAT codes", () => {
    const result = billyParser.parseSource!(
      makeSource({
        "organization.json": JSON.stringify({ id: "org-1", name: "Test" }),
        "accounts.json": JSON.stringify([
          { id: "a1", accountNo: 1000, name: "Revenue", natureId: "revenue", taxRateId: "rate-25" },
          { id: "a2", accountNo: 1810, name: "Office", natureId: "expense", taxRateId: "rate-25" },
          { id: "a3", accountNo: 1230, name: "EU Services", natureId: "expense", taxRateId: "rate-eu" },
        ]),
        "tax-rates.json": JSON.stringify([
          { id: "rate-25", name: "25%", rate: 0.25 },
          { id: "rate-eu", name: "0% Services EU", rate: 0 },
        ]),
      }),
    );

    expect(result.ok).toBe(true);
    const chart = result.source!.chartOfAccounts;

    const revenue = chart.find((a) => a.accountNo === "1000")!;
    expect(revenue.defaultVatCode).toBe("DK_SALE_25");

    const expense = chart.find((a) => a.accountNo === "1810")!;
    expect(expense.defaultVatCode).toBe("DK_PURCHASE_25");

    const eu = chart.find((a) => a.accountNo === "1230")!;
    expect(eu.defaultVatCode).toBe("EU_SERVICE_REVERSE_CHARGE");
  });

  test("is a chart-only import when no balances.json is present", () => {
    const result = billyParser.parseSource!(
      makeSource({
        "organization.json": JSON.stringify({ id: "org-1", name: "Test" }),
        "accounts.json": JSON.stringify([
          { id: "a1", accountNo: 1000, name: "Revenue", natureId: "revenue" },
        ]),
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.source!.cutOverDate).toBe("");
    expect(result.source!.openingBalances.length).toBe(0);
  });

  test("parses opening balances from balances.json", () => {
    const result = billyParser.parseSource!(
      makeSource({
        "organization.json": JSON.stringify({ id: "org-1", name: "Test" }),
        "accounts.json": JSON.stringify([
          { id: "a1", accountNo: 5710, name: "Bank", natureId: "asset" },
          { id: "a2", accountNo: 7310, name: "Creditors", natureId: "liability" },
        ]),
        "balances.json": JSON.stringify({
          cutOverDate: "2026-01-01",
          balances: [
            { accountNo: "5710", balance: 50000 },
            { accountNo: "7310", balance: -30000 },
          ],
        }),
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.source!.cutOverDate).toBe("2026-01-01");
    expect(result.source!.openingBalances.length).toBe(2);

    const bank = result.source!.openingBalances.find((b) => b.accountNo === "5710")!;
    expect(bank.debitAmount).toBe(50000);

    const cred = result.source!.openingBalances.find((b) => b.accountNo === "7310")!;
    expect(cred.creditAmount).toBe(30000);
  });

  test("skips archived accounts", () => {
    const result = billyParser.parseSource!(
      makeSource({
        "organization.json": JSON.stringify({ id: "org-1", name: "Test" }),
        "accounts.json": JSON.stringify([
          { id: "a1", accountNo: 1000, name: "Active", natureId: "revenue", isArchived: false },
          { id: "a2", accountNo: 9999, name: "Archived", natureId: "expense", isArchived: true },
        ]),
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.source!.chartOfAccounts.length).toBe(1);
    expect(result.source!.chartOfAccounts[0]!.accountNo).toBe("1000");
  });

  test("fails with clear errors when required files are missing", () => {
    const result = billyParser.parseSource!(
      makeSource({
        "organization.json": JSON.stringify({ id: "org-1", name: "Test" }),
      }),
    );

    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("accounts.json"))).toBe(true);
  });

  test("falls back to group natureId when account has none", () => {
    const result = billyParser.parseSource!(
      makeSource({
        "organization.json": JSON.stringify({ id: "org-1", name: "Test" }),
        "accounts.json": JSON.stringify([
          { id: "a1", accountNo: 3000, name: "Misc Expense", natureId: "", groupId: "grp-1" },
        ]),
        "account-groups.json": JSON.stringify([
          { id: "grp-1", name: "Expenses", natureId: "expense", type: "group" },
        ]),
      }),
    );

    expect(result.ok).toBe(true);
    expect(result.source!.chartOfAccounts[0]!.normalizedType).toBe("expense");
  });
});
