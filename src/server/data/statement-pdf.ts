// Minimal deterministic PDF-renderer for the three core statements (#463).
//
// Genbruger samme byte-format-tilgang som `invoice-pdf.ts` (manuel PDF 1.4
// uden eksterne libs). En statement-PDF har én side med:
//   - Header: virksomhedsnavn, CVR, regnskabsår, valuta, dato for udtræk
//   - En tabel med (label, beløb)-rækker — sektion-overskrifter er fede
// Output er byte-identisk for samme input + samme udtrækningsdato — vigtigt
// for at revisorens hash-verifikation virker.
//
// Bevidst minimalistisk: ingen pagination, ingen fontmetric-magi. Helvetica
// + WinAnsi-encoding rækker for de tre kerne-rapporter; det er præcis det
// fakturageneratoren også bruger. Lange labels truncate'es elegant.

const PAGE_WIDTH = 595; // A4 portrait, points
const PAGE_HEIGHT = 842;
const MARGIN_LEFT = 50;
const MARGIN_RIGHT = 50;
const MARGIN_TOP = 50;
const TITLE_FONT_SIZE = 16;
const HEADER_FONT_SIZE = 10;
const BODY_FONT_SIZE = 10;
const SECTION_FONT_SIZE = 11;
const LINE_HEIGHT = 14;

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

function escapePdfText(text: string): string {
  // Escape PDF syntax: \ ( )
  return text.replace(/\\/g, "\\\\").replace(/\(/g, "\\(").replace(/\)/g, "\\)");
}

function encodeWinAnsi(text: string): string {
  // For å, æ, ø, Å, Æ, Ø — map til WinAnsi (CP1252) byte-værdier.
  return text.replace(/[^\x00-\x7F]/g, (char) => {
    const code = char.codePointAt(0)!;
    // De fleste danske tegn er allerede i WinAnsi (CP1252) — Helvetica
    // bruger samme encoding. Direkte octal-escape klarer det.
    if (code <= 0xff) return `\\${code.toString(8).padStart(3, "0")}`;
    // Unicode > 255 — drop til "?" så PDF-en ikke crashes.
    return "?";
  });
}

function drawText(
  x: number,
  y: number,
  text: string,
  size: number,
  font: "F1" | "F2",
): string {
  const escaped = encodeWinAnsi(escapePdfText(text));
  return `BT /${font} ${size} Tf ${x} ${y} Td (${escaped}) Tj ET\n`;
}

function buildContent(input: StatementPdfInput): string {
  const lines: string[] = [];
  let y = PAGE_HEIGHT - MARGIN_TOP;

  // Title (bold)
  lines.push(drawText(MARGIN_LEFT, y, input.title, TITLE_FONT_SIZE, "F2"));
  y -= TITLE_FONT_SIZE + 8;

  // Company header block
  const headerLines = [
    input.company.name,
    input.company.cvr ? `CVR ${input.company.cvr}` : null,
    `Regnskabsår ${input.yearLabel}`,
    `Valuta ${input.company.currency}`,
    `Udtrukket ${input.generatedAtIsoDate}`,
    `Kilde: Rentemester`,
  ].filter((l): l is string => Boolean(l));
  for (const headerLine of headerLines) {
    lines.push(drawText(MARGIN_LEFT, y, headerLine, HEADER_FONT_SIZE, "F1"));
    y -= LINE_HEIGHT;
  }
  y -= LINE_HEIGHT; // empty separator

  // Body rows
  const amountX = PAGE_WIDTH - MARGIN_RIGHT - 100;
  for (const row of input.rows) {
    const isBold = row.kind !== "line";
    const font = isBold ? "F2" : "F1";
    const size = row.kind === "section" ? SECTION_FONT_SIZE : BODY_FONT_SIZE;
    lines.push(drawText(MARGIN_LEFT, y, row.label, size, font));
    if (row.amount) {
      lines.push(drawText(amountX, y, row.amount, size, font));
    }
    if (row.kind === "total") {
      // simple horizontal rule above the total
      lines.push(
        `${MARGIN_LEFT} ${y + LINE_HEIGHT - 2} m ${PAGE_WIDTH - MARGIN_RIGHT} ${y + LINE_HEIGHT - 2} l 0.5 w S\n`,
      );
    }
    y -= LINE_HEIGHT;
    if (y < 60) break; // single-page only — large statements truncate
  }

  return lines.join("");
}

/**
 * Returns a deterministic PDF buffer for a Rentemester-statement (#463).
 *
 * `generatedAtIsoDate` styrer både header-tekst og det embeddede PDF-
 * CreationDate, så en byte-identisk PDF kun afhænger af input + dato.
 */
export function buildStatementPdf(input: StatementPdfInput): Buffer {
  const content = buildContent(input);
  const date = input.generatedAtIsoDate.replace(/-/g, "");
  const pdfDate = `D:${date}000000Z`;
  const producer = "Rentemester deterministic statement renderer";

  // Object table: 1 Catalog, 2 Pages, 3 Page, 4 Contents, 5 F1, 6 F2, 7 Info
  const objects: string[] = [
    "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
    "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
    `3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${PAGE_WIDTH} ${PAGE_HEIGHT}] /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> /Contents 4 0 R >>\nendobj\n`,
    `4 0 obj\n<< /Length ${Buffer.byteLength(content, "binary")} >>\nstream\n${content}endstream\nendobj\n`,
    "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj\n",
    "6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>\nendobj\n",
    `7 0 obj\n<< /Producer (${escapePdfText(producer)}) /Title (${escapePdfText(input.title)}) /CreationDate (${pdfDate}) /ModDate (${pdfDate}) >>\nendobj\n`,
  ];

  let pdf = "%PDF-1.4\n%\xE2\xE3\xCF\xD3\n";
  const offsets: number[] = [0];
  for (const object of objects) {
    offsets.push(Buffer.byteLength(pdf, "binary"));
    pdf += object;
  }
  const xrefOffset = Buffer.byteLength(pdf, "binary");
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (let i = 1; i < offsets.length; i += 1) {
    pdf += `${String(offsets[i]).padStart(10, "0")} 00000 n \n`;
  }
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R /Info 7 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`;
  return Buffer.from(pdf, "binary");
}
