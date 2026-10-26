import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";
import { ensureCompanyDirs } from "../../src/core/paths";
import { buildVatReport, postEuGoodsAcquisitionPurchase } from "../../src/core/vat";
import { projectVatRubric } from "../../src/core/vat-rubric";
import { storeViesValidation } from "../../src/core/vies";
import { renderVatReport } from "../../src/cli-format/vat";
import { vatPositionForPeriod, vatRubrikkerForPeriod } from "../../src/server/data/vat";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerVatTools } from "../../src/mcp/tools/vat";

describe("EU goods acquisition VAT", () => {
  test("keeps booked output VAT as a control total while projecting each foreign VAT rubric once", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-eu-goods-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-eu-goods-inbox-"));
    try {
      const source = join(inbox, "goods.txt"); writeFileSync(source, "Synthetic EU goods");
      const db = openDb(ensureCompanyDirs(root).db); migrate(db); seedAccounts(db);
      const doc = ingestDocument(db, root, source, {
        source: "test", issueDate: "2026-08-01", invoiceNo: "GOODS-1", deliveryDescription: "Synthetic goods",
        amountIncVat: 1000, vatAmount: 0, currency: "DKK",
        sender: { name: "Synthetic EU supplier", address: "Berlin", vatOrCvr: "DE123456789" },
        recipient: { name: "Synthetic buyer", address: "Copenhagen", vatOrCvr: "DK12345678" },
      });
      expect(doc.ok).toBe(true);
      storeViesValidation(db, { vatOrCvr: "DE123456789", valid: true, validatedAt: "2026-08-01T00:00:00.000Z", expiresAt: "2026-12-01T00:00:00.000Z" });
      const input = { transactionDate: "2026-08-01", text: "Synthetic EU purchase", documentId: doc.documentId!, netAmount: 1000, expenseAccountNo: "3010" };
      expect(postEuGoodsAcquisitionPurchase(db, input).ok).toBe(true);

      // Domestic sale: output VAT 250.
      expect(postJournalEntry(db, {
        transactionDate: "2026-08-02", text: "Synthetic domestic sale", documentId: doc.documentId!,
        lines: [
          { accountNo: "2000", debitAmount: 1250 },
          { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
          { accountNo: "1200", creditAmount: 250 },
        ],
      }).ok).toBe(true);
      // EU service reverse charge: output and input VAT 250 each.
      expect(postJournalEntry(db, {
        transactionDate: "2026-08-03", text: "Synthetic EU service", documentId: doc.documentId!,
        lines: [
          { accountNo: "3020", debitAmount: 1000, vatCode: "EU_SERVICE_REVERSE_CHARGE" },
          { accountNo: "4000", debitAmount: 250 },
          { accountNo: "2000", creditAmount: 1000 },
          { accountNo: "1200", creditAmount: 250 },
        ],
      }).ok).toBe(true);
      // Ordinary domestic purchase: input VAT 100.
      expect(postJournalEntry(db, {
        transactionDate: "2026-08-04", text: "Synthetic domestic purchase", documentId: doc.documentId!,
        lines: [
          { accountNo: "3000", debitAmount: 400, vatCode: "DK_PURCHASE_25" },
          { accountNo: "4000", debitAmount: 100 },
          { accountNo: "2000", creditAmount: 500 },
        ],
      }).ok).toBe(true);
      const report = buildVatReport(db, "2026-08-01", "2026-08-31");
      // outputVat remains the complete booked control position: domestic sale
      // plus both foreign reverse-charge postings. The filing projection puts
      // the foreign amounts in their dedicated rubrics exactly once.
      expect(report).toMatchObject({
        outputVat: 750,
        inputVat: 600,
        netVatPayable: 150,
        euGoodsAcquisitionPurchaseBase: 1000,
        euGoodsAcquisitionOutputVat: 250,
        reverseChargePurchaseBase: 1000,
        reverseChargePurchaseOutputVat: 500,
      });
      expect(report.rubrikker).toMatchObject({
        salgsmoms: 250,
        rubrikAVarer: 1000,
        rubrikAYdelser: 1000,
        momsAfVarekobUdland: 250,
        momsAfYdelseskobUdland: 250,
        kobsmoms: 600,
        momsIAlt: 150,
        rubrikBVarerEuSalesList: 0,
        rubrikBVarerIkkeEuSalesList: 0,
        rubrikBYdelser: 0,
        rubrikC: 0,
      });
      expect(report.rubrikker.momsIAlt).toBe(report.netVatPayable);

      // Every delivery surface receives the same canonical payable amount:
      // CLI JSON is the report payload, human CLI uses it for its headline,
      // MCP returns it verbatim, and Cockpit derives its VAT card from it.
      expect(JSON.parse(JSON.stringify(report)).netVatPayable).toBe(150);
      expect(renderVatReport(report as unknown as Record<string, unknown>)).toContain(
        "Du skal betale 150,00 kr. i moms for perioden.",
      );
      const server = new McpServer({ name: "vat-payable-parity", version: "0.0.0" });
      registerVatTools(server);
      const vatReportTool = (server as any)._registeredTools.vat_report;
      const mcp = await vatReportTool.handler(
        { company: root, from: "2026-08-01", to: "2026-08-31" },
        { signal: new AbortController().signal },
      );
      expect(mcp.structuredContent.data.netVatPayable).toBe(150);
      expect(mcp.structuredContent.data.rubrikker.momsIAlt).toBe(150);
      const cockpitPosition = vatPositionForPeriod(db, "2026-08-01", "2026-08-31");
      expect(cockpitPosition.payable).toBe(report.netVatPayable);
      expect(vatRubrikkerForPeriod(db, "2026-08-01", "2026-08-31").momsIAlt)
        .toBe(cockpitPosition.payable);

      // A no-EU-goods period retains the exact established projection shape
      // and values, including deterministic JSON field order.
      const zeroGoodsControl = {
        outputVat: 500,
        inputVat: 350,
        euGoodsAcquisitionPurchaseBase: 0,
        euGoodsAcquisitionOutputVat: 0,
        reverseChargePurchaseOutputVat: 250,
        reverseChargePurchaseBase: 1000,
        foreignReverseChargeSalesBase: 0,
        exemptSalesBase: 0,
        domesticReverseChargeSalesBase: 0,
      };
      expect(projectVatRubric(zeroGoodsControl as any)).toMatchObject(
        {
          salgsmoms: 250,
          momsAfVarekobUdland: 0,
          momsAfYdelseskobUdland: 250,
          kobsmoms: 350,
          momsIAlt: 150,
          rubrikAVarer: 0,
          rubrikAYdelser: 1000,
          rubrikBVarerEuSalesList: 0,
          rubrikBVarerIkkeEuSalesList: 0,
          rubrikBYdelser: 0,
          rubrikC: 0,
        },
      );
      db.close();
    } finally { rmSync(root, { recursive: true, force: true }); rmSync(inbox, { recursive: true, force: true }); }
  });
});
