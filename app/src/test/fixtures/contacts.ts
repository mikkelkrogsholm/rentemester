import type { CompanyContacts } from "../../lib/types";
import { STATEMENT_COMPANY, STATEMENT_FISCAL_YEARS } from "./_shared";

export function contacts(
  over: Partial<CompanyContacts> = {},
): CompanyContacts {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    customers: [
      {
        id: 1,
        name: "Kunde A/S",
        vatOrCvr: "DK87654321",
        email: "faktura@kunde.dk",
        paymentTermsDays: 30,
        defaultCurrency: "DKK",
        address: null,
        phone: null,
        website: null,
        eanNumber: null,
        notes: null,
        openBalance: 0,
        openInvoiceCount: 0,
        overdueCount: 0,
      },
    ],
    vendors: [
      {
        id: 1,
        name: "Leverandør ApS",
        vatOrCvr: "DK11223344",
        defaultExpenseAccount: "3000",
        defaultVatTreatment: "standard",
        address: null,
        email: null,
        phone: null,
        website: null,
        notes: null,
      },
    ],
    ...over,
  };
}
