// Dashboard render-engine: deterministisk HTML fra DESIGN.md-tokens.
//
// Kontrakt:
//   renderDashboard(input: DashboardInput): string
//
// Givet samme input skal output være byte-for-byte identisk. Render-engine
// kalder ikke Date.now(), Math.random(), filsystem eller env. Alle disse
// felter samles af CLI'en (#85) og leveres ind via DashboardInput.
//
// Output er én selvstændig HTML-fil med inline CSS. Source Serif 4 + IBM
// Plex Sans + IBM Plex Mono refereres via Google Fonts <link>, men HTML'en
// rendreres korrekt offline med deterministiske system-fallbacks.
//
// This file is a barrel: the actual render-engine lives under
// ./dashboard/*.ts (one section per file). The public surface — types,
// helpers, per-section renderers, and the outer `renderDashboard` composer
// — is re-exported from here so existing importers (the CLI, the snapshot
// test, downstream features) keep their imports unchanged.

export type {
  DashboardExceptionRow,
  DashboardExceptionsResult,
  DashboardAuditStatus,
  DashboardTaxStatus,
  DashboardEuSalesOssStatus,
  DashboardInput,
  RenderOptions,
} from "./dashboard/_shared";

export { formatDkk } from "./dashboard/_shared";
export { metricCard } from "./dashboard/metrics";
export { auditStatusPill } from "./dashboard/audit";
export { backupStatusPill } from "./dashboard/backup";
export { invoiceTable } from "./dashboard/invoices";
export { renderDashboard } from "./dashboard/page";
