import { documentAttr } from "../../design/document-html";
// "Næste momsfrist" card — countdown to the SKAT VAT filing deadline for the
// CLI-selected period.

import {
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
  // When the company is not VAT-registered the card explains so instead of
  // inventing a period from a non-existent cadence.
  if (input.company.vatPeriodType === null) {
    return `<div ${documentAttr("deadline")}>
  <div>
    <div ${documentAttr("labelSm")}>Næste momsfrist</div>
    <div ${documentAttr("deadlineTitle")}>Ikke momsregistreret</div>
    <div ${documentAttr("deadlineDetail")}><span ${documentAttr("pillSuccess")}>Ingen pligt</span> Selskabet er ikke momsregistreret — ingen momsangivelse eller frist.</div>
  </div>
  <div ${documentAttr("right")}>
    <div ${documentAttr("labelSm")}>Est. nettomoms</div>
    <div ${documentAttr("amountLg")}>—</div>
  </div>
</div>`;
  }
  const vatType: VatPeriodType = input.company.vatPeriodType;
  const validStart = /^(\d{4})-(\d{2})-(\d{2})/.test(input.vatPeriod.periodStart);
  const window = validStart
    ? vatPeriodWindowFor(input.vatPeriod.periodStart, vatType)
    : null;
  const period = {
    label: window ? vatPeriodLabel(window) : "—",
  };
  // The countdown targets the canonical SKAT filing/payment deadline for the
  // company's actual cadence (including monthly special dates and bank days).
  const deadline = window ? window.filingDeadline : null;
  const daysRemaining = deadline ? signedDaysBetween(input.asOfDate, deadline) : 0;
  const errors = input.vatPeriod.errors ?? [];
  let pill: string;
  let detail: string;
  if (errors.length > 0) {
    pill = `<span ${documentAttr("pillWarning")}>Kan ikke beregne</span>`;
    detail = truncate(errors[0]!, 80);
  } else if (!deadline) {
    pill = `<span ${documentAttr("pillWarning")}>Kan ikke beregne</span>`;
    detail = "";
  } else if (daysRemaining < 0) {
    pill = `<span ${documentAttr("pillDanger")}>Forfalden</span>`;
    detail = `${Math.abs(daysRemaining)} dage over`;
  } else if (daysRemaining <= 14) {
    pill = `<span ${documentAttr("pillWarning")}>${daysRemaining} dage tilbage</span>`;
    detail = "";
  } else {
    pill = `<span ${documentAttr("pillSuccess")}>${daysRemaining} dage tilbage</span>`;
    detail = "";
  }
  const deadlineLine = deadline
    ? `<div ${documentAttr("deadlineDetail")}>SKAT-frist: <span ${documentAttr("mono")}>${escapeHtml(deadline)}</span></div>`
    : "";
  const net = input.vatPeriod.netVatPayable;
  const netLabel = net < 0 ? "Til gode" : "Est. nettomoms";
  const netValue = formatDkk(net);
  return `<div ${documentAttr("deadline")}>
  <div>
    <div ${documentAttr("labelSm")}>Næste momsfrist</div>
    <div ${documentAttr("deadlineTitle")}>${escapeHtml(period.label)}</div>
    ${deadlineLine}
    <div ${documentAttr("deadlineDetail")}>${pill} ${escapeHtml(detail)}</div>
  </div>
  <div ${documentAttr("right")}>
    <div ${documentAttr("labelSm")}>${escapeHtml(netLabel)}</div>
    <div ${documentAttr("amountLg")}>${escapeHtml(netValue)}</div>
  </div>
</div>`;
}
