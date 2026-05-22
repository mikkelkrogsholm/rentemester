// Import framework — the worked EXAMPLE parser. Issue #185.
//
// `syntheticCsvParser` is the reference implementation of the `SourceParser`
// contract (./types.ts). It parses a small, INVENTED CSV format — see
// examples/import-synthetic.csv — into a normalised `ImportSource`.
//
// Why a synthetic sample and not a real e-conomic / Billy parser?
// The real e-conomic and Billy export formats can only be implemented
// faithfully against actual export files (the same constraint #173 Dinero
// hit). Shipping a parser that *claims* to read "e-conomic" without a real
// sample would be guesswork. So #185 delivers the FRAMEWORK + the parser
// CONTRACT, proven end-to-end by this synthetic example. The real-format
// e-conomic and Billy parsers are a follow-up: they implement exactly this
// same `SourceParser` interface and need no framework changes.
//
// The synthetic CSV format (deliberately simple, fully documented):
//   - Lines starting with `#` are header directives: `# key: value`.
//     `source` and `cutOverDate` are recognised; `cutOverDate` is required.
//   - One `section,accountNo,name,debit,credit` column header row.
//   - `account,<no>,<name>,,`     -> a chart-of-accounts entry.
//   - `opening,<no>,,<debit>,`    -> an opening-balance debit line (øre).
//   - `opening,<no>,,,<credit>`   -> an opening-balance credit line (øre).
//   Amounts are integer øre, exactly like the rest of Rentemester.

import type { ImportAccount, ImportOpeningBalanceLine, ParseResult, SourceParser } from "./types";
import { dineroParser } from "./dinero";
import { billyParser } from "./billy";

const SYSTEM = "synthetic-csv";
const LABEL = "Synthetic CSV (import-framework example)";

/** Splits a single CSV record on commas. The synthetic format has no quoting. */
function splitRecord(line: string): string[] {
  return line.split(",").map((cell) => cell.trim());
}

/** Parses an integer-øre cell. Empty -> undefined; non-integer -> null (error). */
function parseOre(cell: string): number | undefined | null {
  if (cell === "") return undefined;
  if (!/^-?\d+$/.test(cell)) return null;
  return Number(cell);
}

function parseSyntheticCsv(raw: string): ParseResult {
  const errors: string[] = [];
  if (typeof raw !== "string" || raw.trim().length === 0) {
    return { ok: false, errors: ["import file is empty"] };
  }

  const headers = new Map<string, string>();
  const chartOfAccounts: ImportAccount[] = [];
  const openingBalances: ImportOpeningBalanceLine[] = [];
  let sawColumnHeader = false;

  const lines = raw.split(/\r?\n/);
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!.trim();
    if (line.length === 0) continue;

    // Header directive: `# key: value`.
    if (line.startsWith("#")) {
      const body = line.slice(1).trim();
      const sep = body.indexOf(":");
      if (sep > 0) {
        headers.set(body.slice(0, sep).trim().toLowerCase(), body.slice(sep + 1).trim());
      }
      continue;
    }

    const cells = splitRecord(line);
    const section = (cells[0] ?? "").toLowerCase();

    // The column-header row.
    if (section === "section") {
      sawColumnHeader = true;
      continue;
    }

    if (section === "account") {
      const accountNo = cells[1] ?? "";
      const name = cells[2] ?? "";
      if (!accountNo) {
        errors.push(`line ${i + 1}: account row is missing an accountNo`);
        continue;
      }
      chartOfAccounts.push({ accountNo, name });
      continue;
    }

    if (section === "opening") {
      const accountNo = cells[1] ?? "";
      if (!accountNo) {
        errors.push(`line ${i + 1}: opening row is missing an accountNo`);
        continue;
      }
      const debit = parseOre(cells[3] ?? "");
      const credit = parseOre(cells[4] ?? "");
      if (debit === null) {
        errors.push(`line ${i + 1}: debit '${cells[3]}' is not an integer øre amount`);
        continue;
      }
      if (credit === null) {
        errors.push(`line ${i + 1}: credit '${cells[4]}' is not an integer øre amount`);
        continue;
      }
      if (debit === undefined && credit === undefined) {
        errors.push(`line ${i + 1}: opening row for account '${accountNo}' has no amount`);
        continue;
      }
      const out: ImportOpeningBalanceLine = { accountNo };
      if (debit !== undefined) out.debitAmount = debit;
      if (credit !== undefined) out.creditAmount = credit;
      openingBalances.push(out);
      continue;
    }

    errors.push(`line ${i + 1}: unknown section '${cells[0]}'`);
  }

  if (!sawColumnHeader) {
    errors.push("import file is missing the 'section,accountNo,name,debit,credit' header row");
  }

  const cutOverDate = headers.get("cutoverdate") ?? "";
  if (!cutOverDate) {
    errors.push("import file is missing the '# cutOverDate: YYYY-MM-DD' cut-over date header");
  }

  if (errors.length > 0) return { ok: false, errors };

  const note = headers.get("note");
  return {
    ok: true,
    errors: [],
    source: {
      sourceSystem: SYSTEM,
      cutOverDate,
      chartOfAccounts,
      openingBalances,
      ...(note ? { note } : {}),
    },
  };
}

/**
 * The synthetic-CSV example parser — the reference `SourceParser`
 * implementation. The real e-conomic / Billy parsers (and #173 Dinero)
 * implement this exact same interface.
 */
export const syntheticCsvParser: SourceParser = {
  system: SYSTEM,
  label: LABEL,
  parse: parseSyntheticCsv,
};

/**
 * Registry of available parsers, keyed by `system`. The CLI resolves
 * `--system <id>` against this map. Adding the real e-conomic / Billy parsers
 * later is a one-line registration here — no framework change.
 *
 * Parsers may implement the single-string `parse` (synthetic-csv) or the
 * multi-file `parseSource` (Dinero, #193) shape — `runImportFromSource`
 * dispatches on whichever the selected parser provides.
 */
export const PARSERS: Record<string, SourceParser> = {
  [SYSTEM]: syntheticCsvParser,
  [dineroParser.system]: dineroParser,
  [billyParser.system]: billyParser,
};
