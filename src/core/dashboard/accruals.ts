// Accruals card — open balance-sheet accrual exposure + the count of
// recognition periods that are due/overdue and not yet posted.

import type { AccrualRegisterReport, DueAccrualRecognitionResult } from "../accruals";
import { escapeHtml, formatDkk } from "./_shared";

/**
 * Accruals card — open balance-sheet accrual exposure + the count of
 * recognition periods that are due/overdue and not yet posted.
 */
export function accrualsSection(
  register: AccrualRegisterReport | undefined,
  due: DueAccrualRecognitionResult | undefined,
): string {
  const remainingExposure = register?.totals.remainingAmount ?? 0;
  const accruals = register?.accruals ?? [];
  if (accruals.length === 0) {
    return `<div class="empty-state">Ingen periodeafgrænsningsposter</div>`;
  }
  // "Aktive" = not fully recognised — a fully-recognised accrual no longer
  // carries balance-sheet exposure, so counting it would overstate the card.
  const activeCount = accruals.filter((a) => !a.fullyRecognized).length;
  const dueCount = due?.periods.length ?? 0;
  const dueAmount = due?.totalDueAmount ?? 0;
  const duePill = dueCount > 0
    ? `<span class="pill danger">${dueCount} forfalden${dueCount === 1 ? "" : "e"}</span>`
    : `<span class="pill success">✔ ingen forfaldne</span>`;
  return `<div class="status-row">
    <div>
      <div class="label">Resterende balanceeksponering</div>
      <div class="detail">${activeCount} aktiv${activeCount === 1 ? "" : "e"} periodeafgrænsningsposter</div>
    </div>
    <div class="amount-lg">${escapeHtml(formatDkk(remainingExposure))}</div>
  </div>
  <div class="status-row">
    <div>
      <div class="label">Recognition-perioder der skal bogføres</div>
      <div class="detail">${dueCount > 0 ? `${escapeHtml(formatDkk(dueAmount))} skal indtægts-/omkostningsføres` : "alle forfaldne perioder er bogført"}</div>
    </div>
    <div>${duePill}</div>
  </div>`;
}
