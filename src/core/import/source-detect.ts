/**
 * Source detection for the cockpit's generic file-import.
 *
 * A person migrating to Rentemester drops an export file from their previous
 * accounting system into the cockpit; this registry recognises WHICH system
 * and data type the file is, so the importer can route it without the human
 * having to identify the format. Each module is a thin descriptor — a label, a
 * cheap `detect` predicate over the file name and head, and the `dataType`
 * that decides which importer runs.
 *
 * Adding a new source (e-conomic, Billy, a bank-statement format) is a single
 * `IMPORT_MODULES` entry — no caller changes.
 */

/** The kinds of data a recognised file lands in. */
export type ImportDataType = "contacts";

/** A recognised export format. */
export type ImportModule = {
  /** Stable id, e.g. "dinero-contacts". */
  id: string;
  /** Human label (Danish), shown once a file is recognised. */
  label: string;
  /** Originating system (Danish), e.g. "Dinero". */
  system: string;
  /** Which importer the detected file is routed to. */
  dataType: ImportDataType;
  /**
   * True when this module recognises the file. `detect` must be cheap and read
   * only the head of `content` — it runs on every registered module.
   */
  detect: (fileName: string, content: string) => boolean;
};

/** The outcome of running every module's `detect` over a candidate file. */
export type DetectionResult =
  | { ok: true; module: ImportModule }
  | { ok: false; errors: string[] };

/** First non-empty line of a file, with a leading UTF-8 BOM stripped. */
function firstLine(content: string): string {
  const stripped = content.replace(/^﻿/, "");
  for (const line of stripped.split(/\r?\n/)) {
    if (line.trim().length > 0) return line;
  }
  return "";
}

/**
 * Dinero exports its contact book ("Kontakter") as a semicolon-delimited CSV
 * whose header carries `Kontaktnavn`, `CVR-nummer` and `Kontakttype`. That
 * triple is a strong signature — no other recognised export shares it.
 */
const dineroContactsModule: ImportModule = {
  id: "dinero-contacts",
  label: "Dinero — Kontakter (kunder og leverandører)",
  system: "Dinero",
  dataType: "contacts",
  detect(_fileName, content) {
    const header = firstLine(content)
      .split(";")
      .map((cell) => cell.trim().toLowerCase());
    return (
      header.includes("kontaktnavn") &&
      header.includes("cvr-nummer") &&
      header.includes("kontakttype")
    );
  },
};

/** Every recognised export format. */
export const IMPORT_MODULES: ImportModule[] = [dineroContactsModule];

/**
 * Recognise a candidate import file. Returns the single matching module, or an
 * error — nothing matched, the file was empty, or (defensively) more than one
 * module matched, which would make the routing ambiguous.
 *
 * `modules` defaults to the live registry; it is a parameter so the ambiguous
 * (multi-match) branch can be exercised in tests without a second real source.
 */
export function detectImportSource(
  fileName: string,
  content: string,
  modules: readonly ImportModule[] = IMPORT_MODULES,
): DetectionResult {
  if (typeof content !== "string" || content.trim().length === 0) {
    return { ok: false, errors: ["Filen er tom."] };
  }
  const matches = modules.filter((m) => m.detect(fileName, content));
  if (matches.length === 1) {
    return { ok: true, module: matches[0]! };
  }
  if (matches.length === 0) {
    const supported = modules.map((m) => m.label).join(", ");
    return {
      ok: false,
      errors: [
        "Filen blev ikke genkendt som et understøttet eksportformat. " +
          `Understøttede formater: ${supported}.`,
      ],
    };
  }
  return {
    ok: false,
    errors: [
      "Filen matchede flere kendte formater og kan ikke importeres entydigt.",
    ],
  };
}
