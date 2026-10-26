import { documentAttr } from "../../design/document-html";
// Open-invoices (debitor) table + status pill.

import type { InvoiceListResult, InvoiceListRow } from "../invoice-list";
import { escapeHtml, formatDateShort, formatDkk } from "./_shared";

function invoiceStatusPill(row: InvoiceListRow): string {
  if (row.isOverdue) return `<span ${documentAttr("pillDanger")}>overdue${row.overdueDays > 0 ? ` (${row.overdueDays} d)` : ""}</span>`;
  return `<span ${documentAttr("pillSuccess")}>open</span>`;
}

export function invoiceTable(result: InvoiceListResult, maxRows = 10): string {
  if (result.rows.length === 0) {
    return `<div ${documentAttr("empty")}>Ingen åbne fakturaer</div>`;
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
    return `<tr ${documentAttr("tr")}>
  <td ${documentAttr("cellMono")}>${escapeHtml(row.invoiceNumber)}</td>
  <td ${documentAttr("cell")}>${escapeHtml(customer)}</td>
  <td ${documentAttr("cellAmount")}>${escapeHtml(formatDkk(row.openBalance))}</td>
  <td ${documentAttr("cellAmount")}>${escapeHtml(formatDateShort(row.effectiveDueDate))}</td>
  <td ${documentAttr("cellCenter")}>${invoiceStatusPill(row)}</td>
</tr>`;
  }).join("\n");
  const overflowRow = overflow > 0
    ? `<div ${documentAttr("overflow")}>… og ${overflow} yderligere</div>`
    : "";
  return `<div ${documentAttr("tableScroll")} tabindex="0" role="region" aria-label="Regnskabstabel"><table ${documentAttr("table")}>
  <thead>
    <tr ${documentAttr("tr")}>
      <th ${documentAttr("th")}>Fakturanr.</th>
      <th ${documentAttr("th")}>Kunde</th>
      <th ${documentAttr("thAmount")}>Beløb</th>
      <th ${documentAttr("thAmount")}>Forfald</th>
      <th ${documentAttr("thCenter")}>Status</th>
    </tr>
  </thead>
  <tbody>
${rows}
  </tbody>
</table></div>
${overflowRow}`;
}

// Re-export the internal pill for tests / other call sites that might need it.
export { invoiceStatusPill };
