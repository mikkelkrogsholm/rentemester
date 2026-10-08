import { documentAttr } from "../../design/document-html";
// Audit-chain status pill — rendered in the System-status section.

import { escapeHtml, truncate } from "./_shared";

export function auditStatusPill(ok: boolean, entryCount: number, firstError?: string): string {
  if (ok) {
    return `<span ${documentAttr("pillSuccess")}>✔ OK</span> <span ${documentAttr("muted")}>${escapeHtml(entryCount)} entries</span>`;
  }
  const detail = firstError ? truncate(firstError, 80) : "ukendt fejl";
  return `<span ${documentAttr("pillDanger")}>✘ FEJL</span> <span ${documentAttr("muted")}>${escapeHtml(detail)}</span>`;
}
