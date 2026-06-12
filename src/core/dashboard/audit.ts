// Audit-chain status pill — rendered in the System-status section.

import { escapeHtml, truncate } from "./_shared";

export function auditStatusPill(ok: boolean, entryCount: number, firstError?: string): string {
  if (ok) {
    return `<span class="pill success">✔ OK</span> <span class="muted">${escapeHtml(entryCount)} entries</span>`;
  }
  const detail = firstError ? truncate(firstError, 80) : "ukendt fejl";
  return `<span class="pill danger">✘ FEJL</span> <span class="muted">${escapeHtml(detail)}</span>`;
}
