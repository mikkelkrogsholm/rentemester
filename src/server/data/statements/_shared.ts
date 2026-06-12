// Cross-cutting types and re-exports shared by the per-builder modules in this
// folder. Anything used by 2+ statement builders belongs here; per-builder
// types stay private to their own file.
//
// Note: the bulk of the shared helpers (period resolution, account
// classification, rounding, etc.) live in `../shared.ts` and `../archive.ts`
// already — those modules existed before this split and each builder imports
// from them directly.

export type { IncomeStatementLine } from "../archive";
