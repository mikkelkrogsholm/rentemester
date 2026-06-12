// Open-invoices (debitor) table + status pill.

import type { InvoiceListResult, InvoiceListRow } from "../invoice-list";
import { escapeHtml, formatDateShort, formatDkk } from "./_shared";

function invoiceStatusPill(row: InvoiceListRow): string {
  if (row.isOverdue) return `<span class="pill danger">overdue${row.overdueDays > 0 ? ` (${row.overdueDays} d)` : ""}</span>`;
  return `<span class="pill success">open</span>`;
}

export function invoiceTable(result: InvoiceListResult, maxRows = 10): string {
  if (result.rows.length === 0) {
    return `<div class="empty-state">Ingen åbne fakturaer</div>`;
  }
  const sorted = [...result.rows].sort((a, b) => {
    const ad = a.effectiveDueDate ?? "9999-99-99";
    const bd = b.effectiveDueDate ?? "9999-99-99";
    if (ad < bd) return -1;
    if (ad > bd) return 1;
    return a.invoiceNumber.localeCompare(b.invoiceNumber);
  });
  const visible = sorted.slice(0, maxRows);
  const overflow = sorted.length - visible.length;
  const rows = visible.map((row) => {
    const customer = row.customerName ?? row.customerCvr ?? "—";
    return `<tr>
  <td class="mono">${escapeHtml(row.invoiceNumber)}</td>
  <td>${escapeHtml(customer)}</td>
  <td class="amount">${escapeHtml(formatDkk(row.openBalance))}</td>
  <td class="amount mono">${escapeHtml(formatDateShort(row.effectiveDueDate))}</td>
  <td class="center">${invoiceStatusPill(row)}</td>
</tr>`;
  }).join("\n");
  const overflowRow = overflow > 0
    ? `<div class="muted" style="margin-top: var(--space-xs); font-size: 13px;">… og ${overflow} yderligere</div>`
    : "";
  return `<table class="dash-table">
  <thead>
    <tr>
      <th>Fakturanr.</th>
      <th>Kunde</th>
      <th class="amount">Beløb</th>
      <th class="amount">Forfald</th>
      <th class="center">Status</th>
    </tr>
  </thead>
  <tbody>
${rows}
  </tbody>
</table>
${overflowRow}`;
}

// Re-export the internal pill for tests / other call sites that might need it.
export { invoiceStatusPill };
