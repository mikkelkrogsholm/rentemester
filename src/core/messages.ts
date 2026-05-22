// Central dansk besked- og etiketkatalog.
//
// Brugervendt dansk tekst (statusord, alvorlighedsgrader, undtagelsestyper)
// blev tidligere copy-paste'et på tværs af CLI-overflader og dashboardet — og
// var drevet fra hinanden (fx manglede `overdue` ét sted). Dette modul samler
// etiketterne ét sted, så de ikke kan drive fra hinanden igen.
//
// Katalogets accessor-funktioner falder altid tilbage til en defineret
// fallback frem for at vise en rå maskinkode. Kataloget er bygget til at vokse
// trinvist — tilføj nye etiketgrupper her i stedet for at indføre lokale maps.

/** Dansk standard-fallback når en værdi mangler. */
export const FALLBACK_DASH = "—";
/** Dansk fallback når en statuskode er ukendt og en kode ikke kan vises rå. */
export const FALLBACK_UKENDT = "ukendt";

// ===== Fakturastatus =====
// Den komplette status-liste — inkl. `overdue` (forfalden). En tidligere
// kopi i src/cli/invoice.ts udelod `overpaid`/`written_off` ikke, men en kopi
// i cli-format.ts udelod `overdue`. Den fulde liste her er den korrekte. (#316)
const INVOICE_STATUS_DA: Record<string, string> = {
  open: "åben",
  paid: "betalt",
  credited: "krediteret",
  refunded: "refunderet",
  overpaid: "overbetalt",
  written_off: "afskrevet",
  overdue: "forfalden",
};

/**
 * Dansk etiket for en fakturastatus. Ukendt kode returneres uændret, så
 * kaldere kan vælge deres egen yderste fallback.
 */
export function invoiceStatusDa(status: string): string {
  return INVOICE_STATUS_DA[status] ?? status;
}

// ===== Banktransaktionens afstemningsstatus =====
const LEDGER_STATUS_DA: Record<string, string> = {
  matched: "afstemt",
  unmatched: "uafstemt",
};

/** Dansk etiket for en banktransaktions afstemningsstatus. */
export function ledgerStatusDa(status: string): string {
  return LEDGER_STATUS_DA[status] ?? status;
}

// ===== Finansposteringens status =====
const JOURNAL_STATUS_DA: Record<string, string> = {
  posted: "bogført",
  reversed: "tilbageført",
  draft: "kladde",
};

/** Dansk etiket for en finansposterings status. */
export function journalStatusDa(status: string): string {
  return JOURNAL_STATUS_DA[status] ?? status;
}

// ===== Alvorlighedsgrad =====
// To overflader viser alvorlighedsgrad forskelligt: CLI'ens exceptions-liste
// bruger korte småskrevne ord (`lav`/`middel`/`høj`), mens dashboardet viser
// store forbogstaver og kender `critical` (`Kritisk`). `severityDa` dækker
// begge via en stil-parameter, så de to overflader stadig rendrer som før.
const SEVERITY_DA_SHORT: Record<string, string> = {
  low: "lav",
  medium: "middel",
  high: "høj",
};
const SEVERITY_DA_TITLE: Record<string, string> = {
  critical: "Kritisk",
  high: "Høj",
  medium: "Mellem",
  low: "Lav",
};

export type SeverityStyle = "short" | "title";

/**
 * Dansk etiket for en alvorlighedsgrad.
 *
 * - `"short"` (standard): småskrevne ord til CLI-lister — `lav`/`middel`/`høj`.
 * - `"title"`: store forbogstaver til dashboardet — inkl. `Kritisk`.
 *
 * Ukendt kode returneres uændret.
 */
export function severityDa(severity: string, style: SeverityStyle = "short"): string {
  const map = style === "title" ? SEVERITY_DA_TITLE : SEVERITY_DA_SHORT;
  return map[severity] ?? severity;
}

// ===== Undtagelsens status =====
const EXCEPTION_STATUS_DA: Record<string, string> = {
  open: "åben",
  resolved: "løst",
};

/** Dansk etiket for en undtagelses status. */
export function exceptionStatusDa(status: string): string {
  return EXCEPTION_STATUS_DA[status] ?? status;
}

// ===== Undtagelsens type =====
// Ejeren ser dashboardet, ikke udvikleren. En rå type-kode som
// `UNMATCHED_BANK_TRANSACTION` læses som systemets interne — så typen vises
// som en almindelig dansk overskrift. (#270)
const EXCEPTION_TYPE_DA: Record<string, string> = {
  UNMATCHED_BANK_TRANSACTION: "Banktransaktion mangler afstemning",
  BANK_BALANCE_GAP: "Afvigelse mellem bogført og faktisk banksaldo",
  MAIL_INTAKE_NO_ATTACHMENT: "Indkommen mail uden vedhæftet bilag",
  MAIL_INTAKE_AMBIGUOUS_METADATA: "Bilag fra mail med uklare oplysninger",
  MAIL_INTAKE_INGEST_BLOCKED: "Mail kunne ikke indlæses",
  ASSET_WRITEOFF_MISSING_DOCUMENTATION: "Aktiv-afskrivning mangler dokumentation",
  ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN: "Aktiv-afskrivning med usikker fradragsret",
};

/**
 * Plain-dansk overskrift for en undtagelsestype — aldrig en rå maskinkode.
 *
 * En ukendt kode humaniseres (separatorer erstattes, første bogstav stort) i
 * stedet for at vise den rå SCREAMING_SNAKE / punkterede identifikator. (#270)
 */
export function exceptionTypeDa(type: string): string {
  const known = EXCEPTION_TYPE_DA[type];
  if (known) return known;
  const words = type.replace(/[_.]+/g, " ").trim().toLowerCase();
  return words.length > 0 ? words.charAt(0).toUpperCase() + words.slice(1) : "Undtagelse";
}
