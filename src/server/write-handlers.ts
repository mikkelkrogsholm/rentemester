// Cockpit write route handlers (#213, slice 1).
//
// Each handler is a thin adapter: it parses route params + body, runs the
// shared `withCompanyMutation` pipeline (which owns the backup lock, the
// confirm gate, actor resolution and the localhost hard-gate), and calls the
// existing `src/core/` bookkeeping function. The Cockpit NEVER reimplements
// bookkeeping — it is a third caller of core, alongside the CLI and MCP.
//
// The implementation lives in the `./write-handlers/` directory split by
// domain (exceptions, bank, invoice, documents, …). This file is a barrel
// that re-exports the public handler surface so `router.ts` (and any other
// caller) continues to import from `./write-handlers` unchanged.

export * from "./write-handlers/exceptions";
export * from "./write-handlers/bank";
export * from "./write-handlers/import-export";
export * from "./write-handlers/recurring";
export * from "./write-handlers/documents";
export * from "./write-handlers/invoice";
export * from "./write-handlers/company";
export * from "./write-handlers/master-data";
export * from "./write-handlers/mileage";
export * from "./write-handlers/assets";
export * from "./write-handlers/payables";
export * from "./write-handlers/budget";
export * from "./write-handlers/gdpr";
export * from "./write-handlers/bilagsmail";
