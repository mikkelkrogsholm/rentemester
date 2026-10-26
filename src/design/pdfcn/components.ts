/** Selected pdfcn Takumi PageHeader and Table primitives, adapted to semantic
 * HTML and precompiled StyleX attributes. Original: shadcn-labs/pdfcn (MIT),
 * revision ff5d9f65ac3c591c6b44da8290adf25678151902.
 * https://github.com/shadcn-labs/pdfcn/tree/main/apps/web/registry/bases/takumi/components
 * The original two-column header, subtitle and line/striped table composition
 * are retained; styling is authored only in document.stylex.ts.
 */
import { documentAttr, escapeDocumentText as escape } from "../document-html";

export function PageHeader(props: { title: string; subtitle?: string; rightText?: string; rightSubText?: string }): string {
  return `<header ${documentAttr("pdfHeader")}><div><h1 ${documentAttr("pdfHeaderTitle")}>${escape(props.title)}</h1>${props.subtitle ? `<p ${documentAttr("pdfSubtitle")}>${escape(props.subtitle)}</p>` : ""}</div><div ${documentAttr("right")}>${props.rightText ? `<p ${documentAttr("pdfLabel")}>${escape(props.rightText)}</p>` : ""}${props.rightSubText ? `<p ${documentAttr("pdfSubtitle")}>${escape(props.rightSubText)}</p>` : ""}</div></header>`;
}

export function Table(props: { headers: string[]; rows: string[][]; numericColumns?: number[]; zebraStripe?: boolean }): string {
  const numeric = new Set(props.numericColumns ?? []);
  return `<table ${documentAttr("pdfTable")}><thead><tr ${documentAttr("pdfRow")}>${props.headers.map((text, index) => `<th ${documentAttr(numeric.has(index) ? "pdfThNumber" : "pdfTh")}>${escape(text)}</th>`).join("")}</tr></thead><tbody>${props.rows.map((row, index) => `<tr ${documentAttr(props.zebraStripe && index % 2 === 1 ? "pdfRowStripe" : "pdfRow")}>${row.map((text, column) => `<td ${documentAttr(numeric.has(column) ? "pdfNumber" : column === 0 ? "pdfDescription" : "pdfCell")}>${escape(text)}</td>`).join("")}</tr>`).join("")}</tbody></table>`;
}

export function PageFooter(title: string): string {
  // Counter classes are renderer hooks, not authored styling.
  return `<footer ${documentAttr("pdfFooter")}>${escape(title)} - Side <span class="pageNumber"></span> af <span class="totalPages"></span></footer>`;
}
