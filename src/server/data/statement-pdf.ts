import { documentAttr, escapeDocumentText as escape } from "../../design/document-html";
import { PageHeader, PageFooter } from "../../design/pdfcn/components";
import { renderDocumentPdf } from "../../design/pdf-render";

export type StatementPdfRow = {
  /** \"section\" tegnes fed; \"line\" er normal; \"total\" er fed m. linje over. */
  kind: "section" | "line" | "total";
  label: string;
  /** Pre-formatteret beløb-streng (allerede med decimalkomma + valuta). */
  amount?: string;
};

export type StatementPdfInput = {
  title: string;
  company: {
    name: string;
    cvr: string | null;
    currency: string;
  };
  yearLabel: string;
  generatedAtIsoDate: string;
  rows: StatementPdfRow[];
};

/** Same synchronous buffer contract, with complete automatic pagination. */
export function buildStatementPdf(input: StatementPdfInput): Buffer {
  const html = `<main ${documentAttr("pdfBody")}>${PageHeader({ title: input.title, subtitle: input.company.name, rightText: input.yearLabel, rightSubText: input.company.currency })}<section ${documentAttr("pdfSection")}>${[input.company.cvr ? `CVR ${input.company.cvr}` : null, `Regnskabsår ${input.yearLabel}`, `Valuta ${input.company.currency}`, `Udtrukket ${input.generatedAtIsoDate}`, "Kilde: Rentemester"].filter(Boolean).map(text => `<p ${documentAttr("pdfParagraph")}>${escape(text)}</p>`).join("")}</section><table ${documentAttr("pdfTable")}><tbody>${input.rows.map(row => `<tr ${documentAttr("pdfRow")}><td ${documentAttr(row.kind === "total" ? "pdfTotalLabel" : row.kind === "section" ? "pdfSectionCell" : "pdfCell")}>${escape(row.label)}</td><td ${documentAttr(row.kind === "total" ? "pdfTotal" : "pdfNumber")}>${escape(row.amount ?? "")}</td></tr>`).join("")}</tbody></table></main>`;
  return renderDocumentPdf({ html, title: input.title, date: input.generatedAtIsoDate, footer: PageFooter(input.title) });
}
