import type { CompanyDocuments } from "../../lib/types";
import { STATEMENT_COMPANY } from "./_shared";

export function documents(
  over: Partial<CompanyDocuments> = {},
): CompanyDocuments {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    documents: [
      {
        id: 1,
        documentNo: "DOC-2026-000001",
        source: "dinero-import",
        filename: "2026-Bilag-1.pdf",
        documentType: "purchase_sale",
        supplierName: "Leverandør ApS",
        invoiceNo: "F-1001",
        invoiceDate: "2026-02-01",
        amountIncVat: 1250,
        currency: "DKK",
        status: "ingested",
        voucherRef: "1",
        journalEntryNo: "B-2026-0002",
        journalEntryId: 2,
        journalEntryText: "Køb af kontorartikler",
        journalEntryTotal: 1250,
        hasFile: true,
      },
    ],
    linkedCount: 1,
    unlinkedCount: 0,
    ...over,
  };
}
