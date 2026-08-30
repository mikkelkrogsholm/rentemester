import type { CommandSpec } from "./_shared";

// ===== BANK CLUSTER (#186-189) =====
export const bankSpecs: CommandSpec[] = [
  { key: "bank-account add", usage: "bank-account add --company <path> --name <text> [--slug <slug>] [--bank-name <text>] [--registration-no <regnr>] [--account-no <kontonr>] [--iban <iban>] [--bic <swift>] [--account-owner <name>] [--customer-no <number>] [--currency <ISO>] [--ledger-account <konto>]", description: "Opretter en bankkonto i virksomhedens ledger.", allowedFlags: ["--company", "--name", "--slug", "--bank-name", "--registration-no", "--account-no", "--iban", "--bic", "--account-owner", "--customer-no", "--currency", "--ledger-account"] },
  { key: "bank-account update", usage: "bank-account update --company <path> --account <id|slug> [--name <text>] [--bank-name <text>] [--registration-no <regnr>] [--account-no <kontonr>] [--iban <iban>] [--bic <swift>] [--account-owner <name>] [--customer-no <number>] [--currency <ISO>] [--ledger-account <konto>] [--active true|false]", description: "Auditeret opdatering af en bankkontos ikke-hemmelige betalingsprofil. Ledger-remapping afvises når kontoen allerede har transaktioner.", allowedFlags: ["--company", "--account", "--name", "--bank-name", "--registration-no", "--account-no", "--iban", "--bic", "--account-owner", "--customer-no", "--currency", "--ledger-account", "--active"] },
  { key: "bank-account list", usage: "bank-account list --company <path>", description: "Lister registrerede bankkonti.", allowedFlags: ["--company"] },
  {
    key: "bank import",
    usage: "bank import --company <path> --file <transactions.csv> [--account <id|slug>] [--profile danske-bank]",
    description: "Importerer banktransaktioner fra CSV.",
    allowedFlags: ["--company", "--file", "--account", "--profile"],
    examplePath: "examples/bank-transactions.csv",
    inputNotes: [
      "CSV-headeren skal indeholde mindst kolonnerne: transaction_date, text, amount (valgfri: booking_date, currency, reference, amount_dkk, fx_rate_to_dkk).",
      "Danske header-aliasser accepteres uden --profile: dato/date → transaction_date, tekst/beskrivelse → text, beløb/belob → amount, valuta → currency, ref/bilagsnummer → reference.",
      "Datoer er YYYY-MM-DD. Delimiter (komma eller semikolon) detekteres automatisk fra headeren.",
      "amount er i KRONER (decimal): POSITIVT beløb = penge IND på kontoen (indbetaling), NEGATIVT = penge UD (betaling/gebyr).",
      "--profile <navn> bruges når en banks CSV ikke matcher standardformatet (fx 'danske-bank': semikolon, dd.mm.yyyy-datoer, dansk talformat). Uden --profile antages standardformatet.",
      "--account <id|slug> knytter posterne til en bankkonto oprettet med 'bank-account add'.",
      "Import-idempotens, fingerprint-felter og overlap-reimport er dokumenteret kanonisk i docs/bank-import-idempotency.md.",
    ],
  },
  { key: "bank correction-plan", usage: "bank correction-plan --company <path> --bank-transaction-id <n> --replacement-journal-entry-id <n>", description: "Bygger en read-only, hash-bundet plan til at rette én reverseret bankafstemning.", allowedFlags: ["--company", "--bank-transaction-id", "--replacement-journal-entry-id"] },
  { key: "bank correction-apply", usage: "bank correction-apply --company <path> --bank-transaction-id <n> --replacement-journal-entry-id <n> --expected-reconciliation-id <id> --plan-hash <sha256> --reason <text> --principal user:<id>|service-account:<id> --idempotency-key <key> --confirm yes", description: "Supersederer atomisk præcis den reviewede bankafstemning med en gyldig erstatningsjournal. Actor-attribution og stabil principal holdes adskilt.", allowedFlags: ["--company", "--bank-transaction-id", "--replacement-journal-entry-id", "--expected-reconciliation-id", "--plan-hash", "--reason", "--principal", "--idempotency-key", "--confirm"] },
  { key: "bank direct-payable-plan", usage: "bank direct-payable-plan --company <path> --document-id <n> --bank-transaction-id <n> --bill-date <YYYY-MM-DD> --due-date <YYYY-MM-DD> --expense-account <konto> [--vat-treatment standard|exempt|non_deductible] [--vendor-id <n>] [--note <text>]", description: "Bygger en read-only hash-bundet plan til at flytte et direkte bankkøb til kreditorflow med fakturadato og senere bankdato bevaret.", allowedFlags: ["--company","--document-id","--bank-transaction-id","--bill-date","--due-date","--expense-account","--vat-treatment","--vendor-id","--note"] },
  { key: "bank direct-payable-apply", usage: "bank direct-payable-apply --company <path> --document-id <n> --bank-transaction-id <n> --bill-date <YYYY-MM-DD> --due-date <YYYY-MM-DD> --expense-account <konto> --plan-hash <sha256> --reason <text> --principal user:<id>|service-account:<id> --idempotency-key <key> --confirm yes", description: "Anvender præcis den reviewede direct-bank→payable-plan append-only og afstemmer betalingen på bankdatoen.", allowedFlags: ["--company","--document-id","--bank-transaction-id","--bill-date","--due-date","--expense-account","--vat-treatment","--vendor-id","--note","--plan-hash","--reason","--principal","--idempotency-key","--confirm"] },
  { key: "bank list", usage: "bank list --company <path> [--status all|matched|unmatched] [--from <YYYY-MM-DD>] [--to <YYYY-MM-DD>] [--text-match <text>] [--amount <n>] [--account <id|slug>]", description: "Lister importerede banktransaktioner med filtre for afstemningsstatus.", allowedFlags: ["--company", "--status", "--from", "--to", "--text-match", "--amount", "--account"] },
  { key: "bank suggest-matches", usage: "bank suggest-matches --company <path> [--bank-transaction-id <n>] [--max <n>]", description: "Foreslår deterministiske match mellem uafstemte banktransaktioner og fakturaer/bilag.", allowedFlags: ["--company", "--bank-transaction-id", "--max"] },
  {
    key: "bank link-journal",
    usage: "bank link-journal --company <path> --bank-transaction-id <n> --journal-entry-id <n> --match-method exact-date-amount|settlement-lag-amount|source-reference|manual-review --confirm yes [--source-reference <ref>] [--note <text>]",
    description: "Afstemmer append-only en bankpostering mod en allerede bogført journalpost uden at oprette eller ændre finansposteringer.",
    allowedFlags: ["--company", "--bank-transaction-id", "--journal-entry-id", "--match-method", "--source-reference", "--note", "--confirm"],
    inputNotes: [
      "Kræver actor og den ordrette bekræftelse --confirm yes.",
      "Database-gaten kræver, at bankkontoens ledger-konto er kendt, og at journalens netto-bevægelse på denne konto svarer nøjagtigt til bankbeløbet i DKK.",
      "Koblingen er append-only og er beregnet til verificerede migrerede journalposter; den opretter ingen ny bogføring.",
    ],
  },
  { key: "reconcile bank", usage: "reconcile bank --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--status all|matched|unmatched] [--text-match <text>] [--amount <n>] [--account <id|slug>]", description: "Viser afstemte og uafstemte banktransaktioner med filtre.", allowedFlags: ["--company", "--from", "--to", "--status", "--text-match", "--amount", "--account"] },
  // ===== END BANK CLUSTER (#186-189) =====
];
