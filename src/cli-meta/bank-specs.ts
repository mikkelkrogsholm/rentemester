import type { CommandSpec } from "./_shared";

// ===== BANK CLUSTER (#186-189) =====
export const bankSpecs: CommandSpec[] = [
  { key: "bank-account add", usage: "bank-account add --company <path> --name <text> [--slug <slug>] [--bank-name <text>] [--registration-no <regnr>] [--account-no <kontonr>] [--iban <iban>] [--currency <ISO>] [--ledger-account <konto>]", description: "Opretter en bankkonto i virksomhedens ledger.", allowedFlags: ["--company", "--name", "--slug", "--bank-name", "--registration-no", "--account-no", "--iban", "--currency", "--ledger-account"] },
  { key: "bank-account list", usage: "bank-account list --company <path>", description: "Lister registrerede bankkonti.", allowedFlags: ["--company"] },
  {
    key: "bank import",
    usage: "bank import --company <path> --file <transactions.csv> [--account <id|slug>] [--profile danske-bank]",
    description: "Importerer banktransaktioner fra CSV.",
    allowedFlags: ["--company", "--file", "--account", "--profile"],
    examplePath: "examples/bank-transactions.csv",
    inputNotes: [
      "CSV-headeren skal indeholde mindst kolonnerne: transaction_date, text, amount (valgfri: booking_date, currency, reference, amount_dkk).",
      "Danske header-aliasser accepteres uden --profile: dato/date → transaction_date, tekst/beskrivelse → text, beløb/belob → amount, valuta → currency, ref/bilagsnummer → reference.",
      "Datoer er YYYY-MM-DD. Delimiter (komma eller semikolon) detekteres automatisk fra headeren.",
      "amount er i KRONER (decimal): POSITIVT beløb = penge IND på kontoen (indbetaling), NEGATIVT = penge UD (betaling/gebyr).",
      "--profile <navn> bruges når en banks CSV ikke matcher standardformatet (fx 'danske-bank': semikolon, dd.mm.yyyy-datoer, dansk talformat). Uden --profile antages standardformatet.",
      "--account <id|slug> knytter posterne til en bankkonto oprettet med 'bank-account add'.",
      "Import er idempotent: en allerede importeret post (samme dato/tekst/beløb/valuta) springes over.",
    ],
  },
  { key: "bank list", usage: "bank list --company <path> [--status all|matched|unmatched] [--from <YYYY-MM-DD>] [--to <YYYY-MM-DD>] [--text-match <text>] [--amount <n>] [--account <id|slug>]", description: "Lister importerede banktransaktioner med filtre for afstemningsstatus.", allowedFlags: ["--company", "--status", "--from", "--to", "--text-match", "--amount", "--account"] },
  { key: "bank suggest-matches", usage: "bank suggest-matches --company <path> [--bank-transaction-id <n>] [--max <n>]", description: "Foreslår deterministiske match mellem uafstemte banktransaktioner og fakturaer/bilag.", allowedFlags: ["--company", "--bank-transaction-id", "--max"] },
  { key: "reconcile bank", usage: "reconcile bank --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--status all|matched|unmatched] [--text-match <text>] [--amount <n>] [--account <id|slug>]", description: "Viser afstemte og uafstemte banktransaktioner med filtre.", allowedFlags: ["--company", "--from", "--to", "--status", "--text-match", "--amount", "--account"] },
  // ===== END BANK CLUSTER (#186-189) =====
];
