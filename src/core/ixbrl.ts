// iXBRL generation (#177): deterministic inline-XBRL output for a Danish
// regnskabsklasse-B (micro/small) arsrapport.
//
// `generateIxbrl` turns the assembled {@link AnnualReport} into an XHTML
// document with inline-XBRL (`ix:`) tags. The output is BOUNDED: it maps only
// the declared `IXBRL_TAXONOMY_SUBSET` — a clearly-versioned, fixed set of
// regnskabsklasse-B elements — and emits nothing outside it. This is
// deliberately NOT the full Erhvervsstyrelsen taxonomy; a clearly-scoped,
// versioned, deterministic, well-tested subset is the design goal.
//
// SCOPE: the subset reaches the minimum element set a small-company class-B
// arsrapport needs — income statement (resultatopgorelse), balance sheet
// (balance), the management statement (ledelsespategning) and the
// accounting-policies note (anvendt regnskabspraksis). It is still a NAMED
// SUBSET, versioned via `name`/`version`, and must not be mistaken for full
// taxonomy coverage. Expanding it means bumping `version` and adding elements
// to the relevant section — never silently widening scope.
//
// The output is byte-stable: identical input yields a byte-identical document
// (and identical sha256). No wall-clock, no random ordering, integer-stable
// money formatting via src/core/money.ts.
//
// Conservative by design: the document is a PREPARED arsrapport for the owner
// or advisor to review; it is not a submission to Virk/Erhvervsstyrelsen.

import { createHash } from "node:crypto";
import { formatAmount } from "./money";
import type { AnnualReport } from "./annual-report";

const IXBRL_RULE_ID = "DK-ANNUAL-REPORT-IXBRL-002";

/**
 * The four regnskabsklasse-B statement sections this subset groups elements
 * into. The document renders one section block per value, in this order.
 */
export type IxbrlSection =
  | "company-info"
  | "income-statement"
  | "balance-sheet"
  | "management-statement"
  | "accounting-policies";

/**
 * The xbrli context an element is reported against. `duration` covers a span
 * (the fiscal year — income statement, company info); `instant` covers a point
 * in time (the balance-sheet date).
 */
export type IxbrlContext = "duration" | "instant";

/** A single fact element in the bounded, versioned regnskabsklasse-B subset. */
export type IxbrlTaxonomyElement = {
  /** Namespaced element name, e.g. "ar:Revenue". */
  name: string;
  /** "monetary" -> ix:nonFraction; "text" -> ix:nonNumeric. */
  kind: "monetary" | "text";
  /** Human-readable Danish label rendered next to the fact. */
  label: string;
  /** Which class-B statement section this element belongs to. */
  section: IxbrlSection;
  /** The xbrli context the fact is reported against. */
  context: IxbrlContext;
};

/** The bounded, versioned iXBRL taxonomy subset Rentemester maps. */
export type IxbrlTaxonomySubset = {
  /** Clearly-versioned name, signalling this is a partial subset. */
  name: string;
  /** Semantic version of the subset; bump when elements change. */
  version: string;
  scope: string;
  prefix: string;
  namespace: string;
  elements: IxbrlTaxonomyElement[];
};

/**
 * The declared, bounded regnskabsklasse-B taxonomy subset.
 *
 * Element names are namespaced under the rentemester-local `ar` prefix so the
 * output is unambiguous and never mistaken for an official Erhvervsstyrelsen
 * submission. Kept in lock-step with rules/dk/annual-report.yaml.
 *
 * VERSIONING: `name` makes the partial nature explicit and `version` tracks
 * its evolution. v0.2.0 expanded the v0.1.0 set (company info + 3 P&L totals +
 * 3 balance totals) toward the minimum class-B element set: it adds an
 * itemised income statement, a current/non-current-aware balance, an explicit
 * management-statement fact and an accounting-policies fact. It is STILL a
 * named subset — not the full taxonomy.
 */
export const IXBRL_TAXONOMY_SUBSET: IxbrlTaxonomySubset = {
  name: "Rentemester regnskabsklasse-B iXBRL subset (bounded)",
  version: "0.2.0",
  scope: "regnskabsklasse B (micro/small) — bounded subset",
  prefix: "ar",
  namespace: "urn:rentemester:dk:arsrapport:v1",
  elements: [
    // --- Company info (duration) -------------------------------------------
    { name: "ar:CompanyName", kind: "text", label: "Virksomhedsnavn", section: "company-info", context: "duration" },
    { name: "ar:RegistreredCvr", kind: "text", label: "CVR-nummer", section: "company-info", context: "duration" },
    { name: "ar:ReportingPeriodStartDate", kind: "text", label: "Regnskabsaar start", section: "company-info", context: "duration" },
    { name: "ar:ReportingPeriodEndDate", kind: "text", label: "Regnskabsaar slut", section: "company-info", context: "duration" },
    { name: "ar:ReportingClass", kind: "text", label: "Regnskabsklasse", section: "company-info", context: "duration" },
    // --- Income statement / resultatopgorelse (duration) -------------------
    { name: "ar:Revenue", kind: "monetary", label: "Nettoomsaetning", section: "income-statement", context: "duration" },
    { name: "ar:OtherOperatingExpenses", kind: "monetary", label: "Andre eksterne omkostninger", section: "income-statement", context: "duration" },
    { name: "ar:GrossResult", kind: "monetary", label: "Bruttoresultat", section: "income-statement", context: "duration" },
    { name: "ar:ProfitLossFromOrdinaryOperatingActivities", kind: "monetary", label: "Resultat af ordinaer drift", section: "income-statement", context: "duration" },
    { name: "ar:ProfitLossForYear", kind: "monetary", label: "Aarets resultat", section: "income-statement", context: "duration" },
    // --- Balance sheet / balance (instant — the balance-sheet date) --------
    { name: "ar:NoncurrentAssets", kind: "monetary", label: "Anlaegsaktiver", section: "balance-sheet", context: "instant" },
    { name: "ar:CurrentAssets", kind: "monetary", label: "Omsaetningsaktiver", section: "balance-sheet", context: "instant" },
    { name: "ar:Assets", kind: "monetary", label: "Aktiver i alt", section: "balance-sheet", context: "instant" },
    { name: "ar:ContributedCapital", kind: "monetary", label: "Selskabskapital og reserver", section: "balance-sheet", context: "instant" },
    { name: "ar:RetainedEarnings", kind: "monetary", label: "Overfoert resultat", section: "balance-sheet", context: "instant" },
    { name: "ar:Equity", kind: "monetary", label: "Egenkapital i alt", section: "balance-sheet", context: "instant" },
    { name: "ar:Liabilities", kind: "monetary", label: "Gaeldsforpligtelser i alt", section: "balance-sheet", context: "instant" },
    { name: "ar:LiabilitiesAndEquity", kind: "monetary", label: "Passiver i alt", section: "balance-sheet", context: "instant" },
    // --- Management statement / ledelsespategning (duration) ---------------
    { name: "ar:ManagementStatement", kind: "text", label: "Ledelsespaategning", section: "management-statement", context: "duration" },
    // --- Accounting policies / anvendt regnskabspraksis (duration) ---------
    { name: "ar:AccountingPolicies", kind: "text", label: "Anvendt regnskabspraksis", section: "accounting-policies", context: "duration" },
  ],
};

/** Section render order + Danish heading for the document body. */
const SECTION_ORDER: { section: IxbrlSection; heading: string }[] = [
  { section: "company-info", heading: "Selskabsoplysninger" },
  { section: "income-statement", heading: "Resultatopgoerelse" },
  { section: "balance-sheet", heading: "Balance" },
  { section: "management-statement", heading: "Ledelsespaategning" },
  { section: "accounting-policies", heading: "Anvendt regnskabspraksis" },
];

export type GenerateIxbrlResult = {
  ok: boolean;
  appliedRules: string[];
  /** The full XHTML+iXBRL document. Empty string on failure. */
  xhtml: string;
  /** sha256 of `xhtml`, for byte-stability checks and manifests. */
  sha256: string;
  /**
   * The resolved facts, in declared subset order. Exposed so callers (and the
   * golden test) can round-trip the document against the values that produced
   * it without re-parsing. Empty on failure.
   */
  facts: ResolvedFact[];
  /** The taxonomy subset name + version this document was generated against. */
  taxonomy: { name: string; version: string };
  errors: string[];
};

function escapeXml(value: string | number | null | undefined): string {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

/** A single declared fact, with the value Rentemester resolved for it. */
export type ResolvedFact = {
  element: IxbrlTaxonomyElement;
  /** Already-stringified value (formatted money or text). */
  value: string;
};

/**
 * Render one inline-XBRL fact row. Monetary facts use ix:nonFraction with the
 * element's context and the DKK unit; text facts use ix:nonNumeric.
 */
function renderFactRow(fact: ResolvedFact): string {
  const { element, value } = fact;
  const safeValue = escapeXml(value);
  if (element.kind === "monetary") {
    return (
      `    <tr><th>${escapeXml(element.label)}</th>` +
      `<td><ix:nonFraction name="${escapeXml(element.name)}" ` +
      `contextRef="${escapeXml(element.context)}" unitRef="DKK" decimals="2">${safeValue}` +
      `</ix:nonFraction></td></tr>`
    );
  }
  return (
    `    <tr><th>${escapeXml(element.label)}</th>` +
    `<td><ix:nonNumeric name="${escapeXml(element.name)}" ` +
    `contextRef="${escapeXml(element.context)}">${safeValue}</ix:nonNumeric></td></tr>`
  );
}

/**
 * Resolve every declared taxonomy element to a fact value from the annual
 * report. Returns errors if a declared element cannot be resolved — a guard
 * against drift between the taxonomy subset and this mapping.
 */
function resolveFacts(report: AnnualReport): { facts: ResolvedFact[]; errors: string[] } {
  const errors: string[] = [];
  // Money is rendered via the integer-ore formatter, never raw floats.
  const m = (value: number): string => formatAmount(value) ?? "0.00";

  const pl = report.profitAndLoss;
  const bs = report.balanceSheet;
  // The balance sheet splits equity into a carried-in part (`equity.total`) and
  // the un-closed period result (`periodResult`). Total equity is their sum;
  // it must reconcile to assets − liabilities on a balanced sheet.
  const totalEquity = bs.equity.total + bs.periodResult;
  // Gross result = revenue − other external costs (the only P&L lines this
  // micro/small slice models). Ordinary operating result mirrors it: the slice
  // has no financial items, so both equal the year's result here. They are
  // still distinct declared elements so the document keeps the class-B shape.
  const grossResult = pl.totalIncome - pl.totalExpense;

  // The accounting-policies fact is sourced from the notes skeleton so the
  // iXBRL stays in lock-step with the assembled report. Falls back to a clear
  // placeholder if the (bounded) skeleton ever drops the note.
  const policiesNote = report.notes.find((n) => n.id === "accounting-policies");
  const accountingPolicies =
    policiesNote?.body ??
    "Anvendt regnskabspraksis udfyldes af ejer eller revisor.";

  const values: Record<string, string> = {
    "ar:CompanyName": report.company.name,
    "ar:RegistreredCvr": report.company.cvr,
    "ar:ReportingPeriodStartDate": report.fiscalYearStart,
    "ar:ReportingPeriodEndDate": report.fiscalYearEnd,
    "ar:ReportingClass": report.regnskabsklasse,
    "ar:Revenue": m(pl.totalIncome),
    "ar:OtherOperatingExpenses": m(pl.totalExpense),
    "ar:GrossResult": m(grossResult),
    "ar:ProfitLossFromOrdinaryOperatingActivities": m(grossResult),
    "ar:ProfitLossForYear": m(report.aretsResultat),
    "ar:NoncurrentAssets": m(noncurrentAssets(bs)),
    "ar:CurrentAssets": m(currentAssets(bs)),
    "ar:Assets": m(bs.totalAssets),
    "ar:ContributedCapital": m(bs.equity.total),
    "ar:RetainedEarnings": m(bs.periodResult),
    "ar:Equity": m(totalEquity),
    "ar:Liabilities": m(bs.liabilities.total),
    "ar:LiabilitiesAndEquity": m(bs.totalLiabilitiesAndEquity),
    "ar:ManagementStatement": report.ledelsespategning.text,
    "ar:AccountingPolicies": accountingPolicies,
  };

  const facts: ResolvedFact[] = [];
  // Iterate the declared subset so the fact order is fixed and bounded.
  for (const element of IXBRL_TAXONOMY_SUBSET.elements) {
    const value = values[element.name];
    if (value === undefined) {
      errors.push(`iXBRL: declared element ${element.name} has no mapping`);
      continue;
    }
    facts.push({ element, value });
  }
  return { facts, errors };
}

/**
 * Sum the assets the balance-sheet classifier marked as non-current
 * ("noncurrent"). The #176 balance sheet does not split current/non-current
 * at the section level, so the split is derived per line from its type:
 * `asset` lines whose account section is the long-lived part. With the
 * micro/small chart of accounts every asset line is currently a current
 * asset, so this conservatively reports 0 non-current and the full total as
 * current — but the elements stay distinct so the class-B shape is intact.
 */
function noncurrentAssets(_bs: AnnualReport["balanceSheet"]): number {
  // The #176 balance sheet exposes only a flat asset section; without a
  // current/non-current marker we cannot reliably split it. Report 0 here so
  // the slice never overstates non-current assets, and surface the whole
  // amount under current assets. This is a documented limitation of the
  // bounded subset, not a silent guess.
  return 0;
}

function currentAssets(bs: AnnualReport["balanceSheet"]): number {
  // All assets are treated as current (see noncurrentAssets); the two always
  // sum to totalAssets so the balance still reconciles.
  return bs.totalAssets - noncurrentAssets(bs);
}

/**
 * Generate a deterministic iXBRL (inline-XBRL) XHTML document for an assembled
 * arsrapport.
 *
 * Refuses to run on a failed annual report — there is no reportable arsrapport
 * to tag. The document declares only the bounded, versioned micro/small
 * taxonomy subset.
 */
export function generateIxbrl(report: AnnualReport): GenerateIxbrlResult {
  const taxonomy = {
    name: IXBRL_TAXONOMY_SUBSET.name,
    version: IXBRL_TAXONOMY_SUBSET.version,
  };
  const fail = (errors: string[]): GenerateIxbrlResult => ({
    ok: false,
    appliedRules: [IXBRL_RULE_ID],
    xhtml: "",
    sha256: "",
    facts: [],
    taxonomy,
    errors,
  });

  if (!report.ok) {
    return fail([
      "cannot generate iXBRL: the annual report did not pass its prerequisites",
      ...report.errors,
    ]);
  }

  const cvrDigits = report.company.cvr.replace(/^DK/, "");
  if (!/^\d{8}$/.test(cvrDigits)) {
    return fail([
      "cannot generate iXBRL: a registered 8-digit CVR is required for the xbrli context",
    ]);
  }

  const { facts, errors } = resolveFacts(report);
  if (errors.length > 0) {
    return fail(errors);
  }

  const ns = IXBRL_TAXONOMY_SUBSET.namespace;

  // Render the declared facts grouped by section, in the fixed SECTION_ORDER.
  // Each section is a self-contained <table> under a Danish <h2> heading.
  const sectionBlocks = SECTION_ORDER.map(({ section, heading }) => {
    const rows = facts
      .filter((f) => f.element.section === section)
      .map(renderFactRow)
      .join("\n");
    return [`    <h2>${escapeXml(heading)}</h2>`, "    <table>", rows, "    </table>"].join("\n");
  }).join("\n");

  const noteRows = report.notes
    .map(
      (note) =>
        `    <tr><th>${escapeXml(note.title)}</th><td>${escapeXml(note.body)}</td></tr>`,
    )
    .join("\n");

  // The iXBRL hidden header carries the two xbrli contexts and the DKK unit:
  //  - `duration` spans the fiscal year (income statement, company info);
  //  - `instant` is the balance-sheet date (the fiscal-year end).
  // The context dates come straight from the (already final) fiscal year, so
  // the document is fully determined by its input.
  const xhtml = [
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<html xmlns="http://www.w3.org/1999/xhtml"',
    '      xmlns:ix="http://www.xbrl.org/2013/inlineXBRL"',
    '      xmlns:xbrli="http://www.xbrl.org/2003/instance"',
    `      xmlns:ar="${escapeXml(ns)}">`,
    "  <head>",
    "    <title>Arsrapport (regnskabsklasse B) — forberedt af Rentemester</title>",
    `    <meta name="rentemester-ixbrl-taxonomy" content="${escapeXml(taxonomy.name)} v${escapeXml(taxonomy.version)}"/>`,
    "  </head>",
    "  <body>",
    '    <div style="display:none">',
    '      <ix:header>',
    '        <ix:references/>',
    '        <ix:resources>',
    '          <xbrli:context id="duration">',
    "            <xbrli:entity>",
    `              <xbrli:identifier scheme="http://www.cvr.dk">${escapeXml(cvrDigits)}</xbrli:identifier>`,
    "            </xbrli:entity>",
    "            <xbrli:period>",
    `              <xbrli:startDate>${escapeXml(report.fiscalYearStart)}</xbrli:startDate>`,
    `              <xbrli:endDate>${escapeXml(report.fiscalYearEnd)}</xbrli:endDate>`,
    "            </xbrli:period>",
    "          </xbrli:context>",
    '          <xbrli:context id="instant">',
    "            <xbrli:entity>",
    `              <xbrli:identifier scheme="http://www.cvr.dk">${escapeXml(cvrDigits)}</xbrli:identifier>`,
    "            </xbrli:entity>",
    "            <xbrli:period>",
    `              <xbrli:instant>${escapeXml(report.fiscalYearEnd)}</xbrli:instant>`,
    "            </xbrli:period>",
    "          </xbrli:context>",
    '          <xbrli:unit id="DKK">',
    "            <xbrli:measure>iso4217:DKK</xbrli:measure>",
    "          </xbrli:unit>",
    "        </ix:resources>",
    "      </ix:header>",
    "    </div>",
    `    <h1>Arsrapport ${escapeXml(report.fiscalYearStart)} — ${escapeXml(report.fiscalYearEnd)}</h1>`,
    "    <p>Regnskabsklasse B (micro/small). Forberedt af Rentemester; " +
      "ejer eller revisor gennemgar og indberetter.</p>",
    `    <p>iXBRL-taksonomi: ${escapeXml(taxonomy.name)} v${escapeXml(taxonomy.version)} ` +
      "— et afgraenset udsnit, ikke den fulde Erhvervsstyrelsen-taksonomi.</p>",
    sectionBlocks,
    "    <h2>Noter (skelet)</h2>",
    "    <table>",
    noteRows,
    "    </table>",
    "  </body>",
    "</html>",
    "",
  ].join("\n");

  return {
    ok: true,
    appliedRules: [IXBRL_RULE_ID],
    xhtml,
    sha256: createHash("sha256").update(xhtml).digest("hex"),
    facts,
    taxonomy,
    errors: [],
  };
}
