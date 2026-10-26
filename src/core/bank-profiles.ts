// ===== BANK CLUSTER (#186,#188,#189) =====
/**
 * Named CSV import profiles for `bank import --profile <name>`.
 *
 * The generic CSV importer in `bank.ts` parses a file structurally and resolves
 * the three required columns via Danish header aliases. It deliberately refuses
 * to guess the D/M order of a `dd?dd?dddd` date when both components are <= 12,
 * because that order is genuinely ambiguous for an unknown CSV. A real Danske
 * Bank export uses `dd.mm.yyyy`, so a large share of its rows hit that refusal
 * and the whole batch is rejected.
 *
 * A profile resolves the ambiguity by *declaring* the format instead of
 * guessing: it pins the delimiter, encoding, date order and an explicit
 * column->field map. This is a per-profile declaration, never a global change
 * to the generic parser's conservative behaviour.
 */

/** Canonical bank-transaction fields a profile column can map to. */
export type ProfileFieldName =
  | "transaction_date"
  | "booking_date"
  | "text"
  | "amount"
  | "currency"
  | "reference"
  | "amount_dkk"
  | "fx_rate_to_dkk"
  | "counterparty_name"
  | "counterparty_account"
  | "message"
  | "archive_reference"
  | "customer_reference"
  | "balance_after";

/** Day/month ordering of a non-ISO date in the source file. */
export type ProfileDateOrder = "dmy" | "mdy" | "ymd" | "iso";

export type BankImportProfile = {
  name: string;
  /** Human label, e.g. "Danske Bank account export". */
  label: string;
  /** Pinned field delimiter. */
  delimiter: string;
  /**
   * Source encoding. "utf8" tolerates and strips a UTF-8 BOM; "windows-1252"
   * decodes legacy Danish exports. The importer falls back gracefully.
   */
  encoding: "utf8" | "windows-1252";
  /** Declared day/month order for non-ISO dates. */
  dateOrder: ProfileDateOrder;
  /**
   * Explicit source-column -> canonical-field map. Column keys are matched
   * case-insensitively against the normalised header: lower-cased, combining
   * diacritics stripped (NFD), whitespace collapsed to single spaces. Note
   * that Danish `ø`/`å` are single code points and survive normalisation, so
   * keys keep them (e.g. `beløb`). This replaces alias guessing entirely for a
   * profiled file.
   */
  columns: Record<string, ProfileFieldName>;
  /**
   * Declared row order in the source file. It is evidence, not a heuristic:
   * the final running balance is resolved from this persisted direction when
   * several statement rows have the same date.
   */
  statementOrder?: "ascending" | "descending";
};

/**
 * Danske Bank account export.
 *
 *  - 21-column-style semicolon-delimited file, UTF-8 with BOM.
 *  - Dates are `dd.mm.yyyy`; the profile declares `dmy` so all rows normalise.
 *  - `Dato` is the posting date -> transaction_date; `Rentedato` is the value
 *    date -> booking_date. Settled explicitly here rather than via the generic
 *    `rentedato` alias, which would otherwise also claim `Dato`.
 *  - Extra columns (#188) — counterparty, message, references — and the
 *    running balance `Saldo` (#189) are mapped so no source signal is lost.
 */
const DANSKE_BANK: BankImportProfile = {
  name: "danske-bank",
  label: "Danske Bank account export",
  delimiter: ";",
  encoding: "utf8",
  dateOrder: "dmy",
  columns: {
    dato: "transaction_date",
    rentedato: "booking_date",
    tekst: "text",
    "beløb": "amount",
    valuta: "currency",
    saldo: "balance_after",
    afsender: "counterparty_name",
    "oprindelig afsender": "counterparty_name",
    modtagerkonto: "counterparty_account",
    besked: "message",
    arkivreference: "archive_reference",
    kundereference: "customer_reference",
  },
  // The export lists most recent statement rows first. Keeping that fact on
  // every imported row lets readers resolve same-day closing balances without
  // ever treating SQLite insertion ids as banking chronology.
  statementOrder: "descending",
};

const PROFILES: Record<string, BankImportProfile> = {
  [DANSKE_BANK.name]: DANSKE_BANK,
};

/** Returns the profile for `name`, or null when it is not registered. */
export function getBankProfile(name: string): BankImportProfile | null {
  return PROFILES[name.trim().toLowerCase()] ?? null;
}

/** Names of all registered profiles, for error messages and help text. */
export function listBankProfileNames(): string[] {
  return Object.keys(PROFILES).sort();
}
// ===== END BANK CLUSTER (#186,#188,#189) =====
