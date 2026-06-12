// Per-company operational views for the cockpit (#320).
//
// Split out of `server/data.ts` by #320. The year-aware journal, bank, VAT,
// documents, invoices, contacts, obligations and cashflow views, plus the
// read-only company-settings view and the CVR sync. Every figure is computed
// by an existing core function or a shared data helper — no business logic is
// duplicated and nothing here mutates a ledger (CVR sync aside, which goes
// through the core `syncCompanyFromCvr`). Behaviour is unchanged from the
// pre-split `server/data.ts`. Money is kroner throughout.

export * from "./company-views/company-settings";
export * from "./company-views/journal";
export * from "./company-views/bank";
export * from "./company-views/vat";
export * from "./company-views/documents";
export * from "./company-views/invoices";
export * from "./company-views/contacts";
export * from "./company-views/obligations";
export * from "./company-views/cashflow";
export * from "./company-views/recurring";
export * from "./company-views/mileage";
export * from "./company-views/assets";
