// "Næste momsfrist" card — countdown to the SKAT VAT filing deadline for the
// CLI-selected period.

import {
  DEFAULT_VAT_PERIOD_TYPE,
  type VatPeriodType,
  vatPeriodLabel,
  vatPeriodWindowFor,
} from "../periods";
import {
  escapeHtml,
  formatDkk,
  signedDaysBetween,
  truncate,
  type DashboardInput,
} from "./_shared";

export function deadlineSection(input: DashboardInput): string {
  // The "Næste momsfrist" box must describe the VAT period the CLI selected —
  // the earliest unreported period that carries activity — NOT the calendar
  // period today falls in. The CLI delivers that period as `vatPeriod`; the
  // render-engine keys the label/deadline off `vatPeriod.periodStart` so the
  // box always agrees with the figure shown beside it. (#281)
  //
  // #299: the period window + label + filing deadline follow the company's
  // real VAT cadence (`vatPeriodType`) — a half-yearly filer sees "1. halvår
  // 2026" with the half-year deadline, not a quarter. For a `quarter` company
  // the window/label/deadline are byte-identical to the historical behaviour.
  const vatType: VatPeriodType =
    input.company.vatPeriodType ?? DEFAULT_VAT_PERIOD_TYPE;
  const validStart = /^(\d{4})-(\d{2})-(\d{2})/.test(input.vatPeriod.periodStart);
  const window = validStart
    ? vatPeriodWindowFor(input.vatPeriod.periodStart, vatType)
    : null;
  const period = {
    label: window ? vatPeriodLabel(window) : "—",
  };
  // The countdown targets the real SKAT filing/payment deadline — the 1st of
  // the third month after the period ends — for the company's actual cadence.
  const deadline = window ? window.filingDeadline : null;
  const daysRemaining = deadline ? signedDaysBetween(input.asOfDate, deadline) : 0;
  const errors = input.vatPeriod.errors ?? [];
  let pill: string;
  let detail: string;
  if (errors.length > 0) {
    pill = `<span class="pill warning">Kan ikke beregne</span>`;
    detail = truncate(errors[0]!, 80);
  } else if (!deadline) {
    pill = `<span class="pill warning">Kan ikke beregne</span>`;
    detail = "";
  } else if (daysRemaining < 0) {
    pill = `<span class="pill danger">Forfalden</span>`;
    detail = `${Math.abs(daysRemaining)} dage over`;
  } else if (daysRemaining <= 14) {
    pill = `<span class="pill warning">${daysRemaining} dage tilbage</span>`;
    detail = "";
  } else {
    pill = `<span class="pill success">${daysRemaining} dage tilbage</span>`;
    detail = "";
  }
  const deadlineLine = deadline
    ? `<div class="muted" style="font-size: 13px; margin-top: var(--space-xxs);">SKAT-frist: <span class="mono">${escapeHtml(deadline)}</span></div>`
    : "";
  const net = input.vatPeriod.netVatPayable;
  const netLabel = net < 0 ? "Til gode" : "Est. nettomoms";
  const netValue = formatDkk(net);
  return `<div class="deadline-card">
  <div>
    <div class="label-sm">Næste momsfrist</div>
    <div class="headline" style="font-size: 18px;">${escapeHtml(period.label)}</div>
    ${deadlineLine}
    <div class="muted" style="font-size: 13px; margin-top: var(--space-xxs);">${pill} ${escapeHtml(detail)}</div>
  </div>
  <div style="text-align: right;">
    <div class="label-sm">${escapeHtml(netLabel)}</div>
    <div class="amount-lg">${escapeHtml(netValue)}</div>
  </div>
</div>`;
}
