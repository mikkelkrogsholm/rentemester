// ===========================================================================
// CLI FORMATTING — re-export barrel (#322)
// ---------------------------------------------------------------------------
// The CLI formatting code is split per result domain under `src/cli-format/`:
//   - cli-format/common.ts    — OutputFormat, generic structured-result
//                               rendering, shared value/format helpers
//   - cli-format/report.ts    — the human-report/write dispatch
//                               (HumanReportKind, renderHumanReport, …)
//   - cli-format/vat.ts       — VAT report, VAT filing, annual report
//   - cli-format/invoice.ts   — invoice status / interest / compensation /
//                               validate / create renderers
//   - cli-format/bank.ts      — bank reconciliation renderer
//   - cli-format/dashboard.ts — trial balance, P&L, balance, exceptions,
//                               retention, backup status, contact lists
//
// This file stays as a thin barrel so every existing
// `import … from "./cli-format"` / `"../cli-format"` keeps resolving
// unchanged — the split is purely internal, no consumer is affected.
// ===========================================================================

export type { OutputFormat } from "./cli-format/common";
export { resolveOutputFormat, printStructuredResult, formatKroner } from "./cli-format/common";

export type { HumanReportKind, HumanWriteKind } from "./cli-format/report";
export { renderHumanReport, emitHumanReport, emitHumanWrite } from "./cli-format/report";

export { renderContactList } from "./cli-format/dashboard";
