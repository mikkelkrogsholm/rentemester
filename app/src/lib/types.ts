// Wire types — the JSON shapes returned by `rentemester serve` (#170).
//
// These mirror `src/server/data.ts` and `src/server/router.ts`. They are kept
// deliberately as a hand-written copy: the SPA is a separate package and does
// not import from the backend's TypeScript sources.
//
// The wire-type surface is split across `./types/<domain>.ts`; this barrel
// re-exports every alias so existing consumers can keep importing from
// `../lib/types`. New code is welcome to import directly from the per-domain
// modules.

export * from "./types/common";
export * from "./types/companies";
export * from "./types/exceptions";
export * from "./types/dashboard";
export * from "./types/statements";
export * from "./types/journal";
export * from "./types/bank";
export * from "./types/vat";
export * from "./types/documents";
export * from "./types/invoices";
export * from "./types/contacts";
export * from "./types/obligations";
export * from "./types/mileage";
export * from "./types/budget";
export * from "./types/payables";
export * from "./types/assets";
export * from "./types/rules";
export * from "./types/retention";
export * from "./types/integrity";
export * from "./types/accounts";
export * from "./types/periods";
export * from "./types/gdpr";
export * from "./types/accruals";
export * from "./types/annual-report";
export * from "./types/bilagsmail";
