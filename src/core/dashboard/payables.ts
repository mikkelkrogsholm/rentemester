import { documentAttr } from "../../design/document-html";
// Creditor card — open and overdue accounts-payable. Symmetric to the
// existing open-invoices (debitor) table.

import type { PayablesListResult } from "../payables";
import { escapeHtml, formatDateShort, formatDkk } from "./_shared";

/**
 * Creditor card — open and overdue accounts-payable. Symmetric to the
 * existing open-invoices (debitor) table.
 */
export function payablesSection(payables: PayablesListResult): string {
  if (payables.count === 0 || payables.rows.length === 0) {
    return `<div ${documentAttr("empty")}>Ingen åbne kreditorposter</div>`;
  }
  const maxRows = 10;
  // buildPayablesList already sorts most-overdue first.
  const visible = payables.rows.slice(0, maxRows);
  const overflow = payables.rows.length - visible.length;
  const rows = visible.map((row) => {
    const supplier = row.supplierName ?? "—";
    const pill = row.isOverdue
      ? `<span ${documentAttr("pillDanger")}>forfalden${row.overdueDays > 0 ? ` (${row.overdueDays} d)` : ""}</span>`
      : `<span ${documentAttr("pillSuccess")}>åben</span>`;
    return `<tr ${documentAttr("tr")}>
  <td ${documentAttr("cellMono")}>${escapeHtml(row.billNo ?? `#${row.payableId}`)}</td>
  <td ${documentAttr("cell")}>${escapeHtml(supplier)}</td>
  <td ${documentAttr("cellAmount")}>${escapeHtml(formatDkk(row.openBalance))}</td>
  <td ${documentAttr("cellAmount")}>${escapeHtml(formatDateShort(row.dueDate))}</td>
  <td ${documentAttr("cellCenter")}>${pill}</td>
</tr>`;
  }).join("\n");
  const overflowRow = overflow > 0
    ? `<div ${documentAttr("overflow")}>… og ${overflow} yderligere</div>`
    : "";
  const summary =
    `<div ${documentAttr("payableSummary")}>` +
    `Åben kreditorgæld i alt <span ${documentAttr("mono")}>${escapeHtml(formatDkk(payables.totalOpenBalance))}</span>` +
    (payables.overdueOpenBalance > 0
      ? ` · heraf overforfalden <span ${documentAttr("mono")}>${escapeHtml(formatDkk(payables.overdueOpenBalance))}</span>`
      : "") +
    `</div>`;
  return `${summary}<table ${documentAttr("table")}>
  <thead>
    <tr ${documentAttr("tr")}>
      <th ${documentAttr("th")}>Bilagsnr.</th>
      <th ${documentAttr("th")}>Leverandør</th>
      <th ${documentAttr("thAmount")}>Åben saldo</th>
      <th ${documentAttr("thAmount")}>Forfald</th>
      <th ${documentAttr("thCenter")}>Status</th>
    </tr>
  </thead>
  <tbody>
${rows}
  </tbody>
</table>
${overflowRow}`;
}
