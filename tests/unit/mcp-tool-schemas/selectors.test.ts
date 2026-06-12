// Tests: src/mcp/tool-runtime.ts, src/mcp/tools (typed payload schemas + confirm envelope)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { startMcpFixture, stopMcpFixture, type StdioMcpClient } from "./_shared";

let companyRoot: string;
let client: StdioMcpClient;

beforeAll(async () => {
  ({ companyRoot, client } = await startMcpFixture());
});

afterAll(async () => {
  await stopMcpFixture(companyRoot, client);
});

describe("#294 — scalar-flag tools carry field descriptions and the documentId|invoiceNumber selector", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  function schemaOf(name: string) {
    const tool = tools.find((t) => t.name === name);
    expect(tool, `tool ${name} not found`).toBeDefined();
    return tool.inputSchema as any;
  }

  test("expense_book flat scalar fields carry descriptions", () => {
    const props = schemaOf("expense_book").properties ?? {};
    for (const field of [
      "documentId",
      "bankTransactionId",
      "expenseAccount",
      "paymentAccount",
      "date",
      "text",
    ]) {
      expect(typeof props[field]?.description, `expense_book.${field} description`).toBe(
        "string",
      );
      expect(
        (props[field]?.description ?? "").length,
        `expense_book.${field} description length`,
      ).toBeGreaterThan(10);
    }
    // date must state the YYYY-MM-DD format.
    expect(props.date?.description).toContain("YYYY-MM-DD");
    // paymentAccount must state the account-2000 default.
    expect(props.paymentAccount?.description).toContain("2000");
  });

  test("invoice_remind / journal_reverse flat scalar fields carry descriptions", () => {
    const remind = schemaOf("invoice_remind").properties ?? {};
    expect(remind.date?.description).toContain("YYYY-MM-DD");
    expect(typeof remind.fee?.description).toBe("string");
    expect(typeof remind.note?.description).toBe("string");

    const reverse = schemaOf("journal_reverse").properties ?? {};
    for (const field of ["entryId", "entryNo", "matchText", "date", "reason"]) {
      expect(typeof reverse[field]?.description, `journal_reverse.${field}`).toBe("string");
    }
    expect(reverse.date?.description).toContain("YYYY-MM-DD");
  });

  test("read tools bank_list / invoice_list / invoice_status / reconcile_bank carry field descriptions", () => {
    const bank = schemaOf("bank_list").properties ?? {};
    expect(typeof bank.status?.description).toBe("string");
    expect(bank.from?.description).toContain("YYYY-MM-DD");
    expect(bank.to?.description).toContain("YYYY-MM-DD");
    expect(typeof bank.account?.description).toBe("string");

    const invList = schemaOf("invoice_list").properties ?? {};
    expect(typeof invList.status?.description).toBe("string");
    expect(invList.from?.description).toContain("YYYY-MM-DD");

    const invStatus = schemaOf("invoice_status").properties ?? {};
    expect(invStatus.asOf?.description).toContain("YYYY-MM-DD");

    const recon = schemaOf("reconcile_bank").properties ?? {};
    expect(recon.from?.description).toContain("YYYY-MM-DD");
    expect(recon.to?.description).toContain("YYYY-MM-DD");
  });

  test("#388 — invoice_find: all fields have describe, amount semantics is documented, and range filters exist", () => {
    const props = schemaOf("invoice_find").properties ?? {};

    // Every searchable field must have a non-empty describe so an agent
    // knows exactly what to send.
    for (const field of ["query", "customer", "invoiceNumber", "asOf"]) {
      const desc: string = props[field]?.description ?? "";
      expect(typeof props[field]?.description, `invoice_find.${field}`).toBe("string");
      expect(desc.length, `invoice_find.${field} non-empty`).toBeGreaterThan(0);
    }

    // The shape of `query` must be unambiguous: which fields does it hit,
    // and is it substring or exact? Without that an agent guesses.
    const queryDesc: string = props.query?.description ?? "";
    expect(queryDesc.toLowerCase()).toMatch(/substring|delstreng|like/);

    // `asOf` must state YYYY-MM-DD so dates aren't sent in the wrong format.
    expect(props.asOf?.description).toContain("YYYY-MM-DD");

    // The exact-match semantics of `amount` MUST be stated explicitly —
    // otherwise an agent using `amount: 10000` to find an invoice around
    // 10,000 kr. gets back an empty list in good faith (silent failure).
    const amountDesc: string = props.amount?.description ?? "";
    expect(amountDesc.toLowerCase()).toMatch(/eksakt|exact/);

    // And the tool MUST expose minAmount/maxAmount for range search so
    // bank-reconciliation flows (where øre-deviations are normal) actually work.
    expect(typeof props.minAmount?.description, "invoice_find.minAmount").toBe("string");
    expect(typeof props.maxAmount?.description, "invoice_find.maxAmount").toBe("string");
  });

  test("#389 — bank_suggest_matches: inputs describe semantics and description documents matching/confidence/truncation", () => {
    const tool = tools.find((t) => t.name === "bank_suggest_matches");
    expect(tool, "bank_suggest_matches not found").toBeDefined();
    const props = schemaOf("bank_suggest_matches").properties ?? {};

    // Both inputs must carry .describe() text with concrete semantics.
    const txDesc: string = props.bankTransactionId?.description ?? "";
    expect(typeof props.bankTransactionId?.description, "bankTransactionId description").toBe(
      "string",
    );
    expect(txDesc.length, "bankTransactionId description non-empty").toBeGreaterThan(20);
    // It must state what happens when omitted, so an agent can decide.
    expect(txDesc.toLowerCase()).toMatch(/omit|udelad/);

    const maxDesc: string = props.max?.description ?? "";
    expect(typeof props.max?.description, "max description").toBe("string");
    expect(maxDesc.length, "max description non-empty").toBeGreaterThan(20);
    // The default value must be stated so an agent can plan around truncation.
    expect(maxDesc.toLowerCase()).toContain("default");
    // The per-transaction vs total scope must be unambiguous.
    expect(maxDesc.toLowerCase()).toMatch(
      /per[- ](bank )?transaction|per[- ]?tx|per transaktion|not total/,
    );

    // The tool description must spell out matching rules + confidence scale +
    // truncation so an agent can set a safe auto-apply threshold.
    const desc: string = (tool.description ?? "").toLowerCase();
    // Confidence scale (0..1, threshold ~0.5) must be visible.
    expect(desc).toContain("confidence");
    expect(desc).toMatch(/0\.\.1|0-1|0 to 1/);
    // The matching signals (amount / invoice number / counterparty name) must be named.
    expect(desc).toContain("amount");
    expect(desc).toMatch(/invoice number|invoice no|fakturanummer/);
    // Ordering of the returned unmatched transactions must be stated.
    expect(desc).toMatch(/order|sorter|sorted|rækkefølge/);
    // Truncation semantics from `max` (jf. #381) must be stated.
    expect(desc).toMatch(/truncat|afkort|begræns/);
  });

  test("#387 — vat_report / vat_eu_sales_list / vat_oss_report from+to carry YYYY-MM-DD describes", () => {
    for (const name of ["vat_report", "vat_eu_sales_list", "vat_oss_report"]) {
      const props = schemaOf(name).properties ?? {};
      expect(props.from?.description, `${name}.from description`).toContain("YYYY-MM-DD");
      expect(props.to?.description, `${name}.to description`).toContain("YYYY-MM-DD");
      // The endpoint-inclusion convention must be visible so agents don't drift by one day.
      expect(
        (props.to?.description ?? "").toLowerCase(),
        `${name}.to states inclusion`,
      ).toMatch(/inclusiv|inclusive/);
    }
  });

  // The documentId|invoiceNumber selector must be visible in BOTH fields.
  const SELECTOR_TOOLS = [
    "invoice_status",
    "invoice_post",
    "invoice_render",
    "invoice_remind",
    "invoice_post_reminder",
    "invoice_claim_interest",
    "invoice_post_interest",
    "invoice_claim_compensation",
    "invoice_post_compensation",
    "invoice_interest_calc",
    "invoice_compensation_calc",
  ];

  for (const name of SELECTOR_TOOLS) {
    test(`${name}: documentId and invoiceNumber both describe the exactly-one selector rule`, () => {
      const props = schemaOf(name).properties ?? {};
      const docDesc: string = props.documentId?.description ?? "";
      const numDesc: string = props.invoiceNumber?.description ?? "";
      expect(docDesc.length, `${name}.documentId description`).toBeGreaterThan(10);
      expect(numDesc.length, `${name}.invoiceNumber description`).toBeGreaterThan(10);
      // The "provide exactly one" rule must be visible in BOTH fields.
      for (const [field, desc] of [
        ["documentId", docDesc],
        ["invoiceNumber", numDesc],
      ] as const) {
        const lc = desc.toLowerCase();
        expect(lc, `${name}.${field} mentions the alternative`).toContain("invoicenumber");
        expect(lc, `${name}.${field} mentions documentId`).toContain("documentid");
        expect(lc, `${name}.${field} states the one-of rule`).toMatch(
          /exactly one|provide (?:either|one)/,
        );
      }
    });
  }
});
