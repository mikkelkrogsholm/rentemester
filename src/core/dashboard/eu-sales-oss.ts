// EU sales / OSS indicator — a LIGHT card. It only renders when there is
// cross-border B2B sales activity or OSS-classified consumer sales in the
// period that need a separate filing; otherwise the caller omits the section.

import { escapeHtml, formatDkk, type DashboardEuSalesOssStatus } from "./_shared";

/**
 * EU sales / OSS indicator — a LIGHT card. It only renders when there is
 * cross-border B2B sales activity or OSS-classified consumer sales in the
 * period that need a separate filing; otherwise the caller omits the section.
 */
export function euSalesOssSection(status: DashboardEuSalesOssStatus): string {
  const rows: string[] = [];
  if (status.euSalesValue > 0 || status.euCustomerCount > 0) {
    rows.push(`<div class="status-row">
    <div>
      <div class="label">EU-salg uden moms (VIES)</div>
      <div class="detail">${status.euCustomerCount} EU-kunde${status.euCustomerCount === 1 ? "" : "r"} — separat liste til SKAT</div>
    </div>
    <div class="amount-lg">${escapeHtml(formatDkk(status.euSalesValue))}</div>
  </div>`);
  }
  if (status.ossConsumerSalesBase > 0) {
    rows.push(`<div class="status-row">
    <div>
      <div class="label">OSS — salg til EU-forbrugere</div>
      <div class="detail">grundlag for separat OSS-angivelse</div>
    </div>
    <div class="amount-lg">${escapeHtml(formatDkk(status.ossConsumerSalesBase))}</div>
  </div>`);
  }
  return rows.join("\n");
}

/** Whether the EU sales / OSS indicator has anything to surface. */
export function hasEuSalesOssActivity(status: DashboardEuSalesOssStatus | undefined): boolean {
  if (!status) return false;
  return (
    status.euSalesValue > 0 ||
    status.euCustomerCount > 0 ||
    status.ossConsumerSalesBase > 0
  );
}
