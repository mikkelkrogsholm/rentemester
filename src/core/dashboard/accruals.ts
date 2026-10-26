import { documentAttr } from "../../design/document-html";
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
    return `<div ${documentAttr("empty")}>Ingen periodeafgrænsningsposter</div>`;
  }
  // "Aktive" = not fully recognised — a fully-recognised accrual no longer
  // carries balance-sheet exposure, so counting it would overstate the card.
  const activeCount = accruals.filter((a) => !a.fullyRecognized).length;
  const dueCount = due?.periods.length ?? 0;
  const dueAmount = due?.totalDueAmount ?? 0;
  const duePill = dueCount > 0
    ? `<span ${documentAttr("pillDanger")}>${dueCount} forfalden${dueCount === 1 ? "" : "e"}</span>`
    : `<span ${documentAttr("pillSuccess")}>✔ ingen forfaldne</span>`;
  return `<div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Resterende balanceeksponering</div>
      <div ${documentAttr("detail")}>${activeCount} aktiv${activeCount === 1 ? "" : "e"} periodeafgrænsningsposter</div>
    </div>
    <div ${documentAttr("amountLg")}>${escapeHtml(formatDkk(remainingExposure))}</div>
  </div>
  <div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Recognition-perioder der skal bogføres</div>
      <div ${documentAttr("detail")}>${dueCount > 0 ? `${escapeHtml(formatDkk(dueAmount))} skal indtægts-/omkostningsføres` : "alle forfaldne perioder er bogført"}</div>
    </div>
    <div>${duePill}</div>
  </div>`;
}
