import type { CommandSpec } from "./_shared";

// ===== PAYABLES / KREDITORSTYRING =====
export const payableSpecs: CommandSpec[] = [
  {
    key: "payable register",
    usage:
      "payable register --company <path> --document-id <n> --bill-date <YYYY-MM-DD> --due-date <YYYY-MM-DD> --expense-account <konto> [--vat-treatment standard|exempt] [--vendor-id <n>] [--note <text>]",
    description:
      "Registrerer et bogført leverandørbilag som en åben kreditorpost (kreditorstyring). Posteringen debiterer udgift + købsmoms og krediterer 7000 Leverandørgæld, så gælden står åben i bøgerne indtil den betales.",
    allowedFlags: ["--company", "--document-id", "--bill-date", "--due-date", "--expense-account", "--vat-treatment", "--vendor-id", "--note"],
    inputNotes: [
      "--document-id binder kreditorposten til et indlæst købsbilag (heltal-id). Et bilag kan kun registreres som kreditorpost én gang.",
      "--bill-date er bilagsdatoen (bogføringsdato for kreditorposteringen); --due-date er forfaldsdatoen gælden afdrages efter i kreditorlisten.",
      "--expense-account: kontonummeret udgiften bogføres på (fx 3000 Software og SaaS).",
      "--vat-treatment: 'standard' løfter 25 % dansk købsmoms af bilaget; 'exempt' bogfører ingen moms. Udelades den, udledes den af bilagets vat_amount (>0 → standard, =0 → exempt).",
      "Kun DKK-bilag understøttes i denne første slice — fremmedvaluta er uden for scope.",
      "Reverse charge / repræsentation hører til 'expense book' og modelleres ikke som kreditorpost.",
    ],
  },
  {
    key: "payable pay",
    usage: "payable pay --company <path> --payable-id <n> --bank-transaction-id <n> [--amount <n>] [--date <YYYY-MM-DD>] [--payment-account <konto>] [--note <text>]",
    description:
      "Matcher en udgående bankbetaling mod en åben kreditorpost. Posteringen debiterer 7000 Leverandørgæld og krediterer bankkontoen, så den åbne kreditorsaldo nedskrives.",
    allowedFlags: ["--company", "--payable-id", "--bank-transaction-id", "--amount", "--date", "--payment-account", "--note"],
    inputNotes: [
      "--payable-id udpeger den åbne kreditorpost (heltal-id fra 'payable register' / 'payable list').",
      "--bank-transaction-id skal pege på en UDGÅENDE (negativ) DKK-bankpost, der ikke allerede er bogført eller anvendt.",
      "--amount er valgfri; udelades den, bruges bankpostens beløb. Angives den, skal den matche bankpostens beløb og må ikke overstige den åbne saldo.",
      "--date er valgfri bogføringsdato YYYY-MM-DD; udelades den, bruges bankpostens dato.",
      "--payment-account: betalingskontoen der krediteres; standard er 2000 (Bank).",
    ],
  },
  {
    key: "payable list",
    usage:
      "payable list --company <path> [--status open|paid|overdue|all] [--as-of <YYYY-MM-DD>] [--supplier <text>] [--vendor-id <n>] [--from <YYYY-MM-DD>] [--to <YYYY-MM-DD>] [--min-days <n>]",
    description:
      "Bygger kreditorlisten: åbne leverandørposter med åben saldo og forfaldsintervaller (forfaldne / ikke-forfaldne), symmetrisk med debitorsiden.",
    allowedFlags: ["--company", "--status", "--as-of", "--supplier", "--vendor-id", "--from", "--to", "--min-days"],
    inputNotes: [
      "--status: 'open' (åbne poster), 'paid' (betalte), 'overdue' (forfaldne åbne), 'all' (standard).",
      "--as-of er skæringsdatoen forfald beregnes mod; udelades den, bruges 1970-01-01 (intet er forfaldent).",
      "--from / --to filtrerer på bilagsdato; --supplier matcher leverandørnavn (delstreng); --vendor-id matcher leverandør-id.",
      "--min-days bruges sammen med --status overdue til kun at vise poster der er mindst så mange dage forfaldne.",
      "Forfaldsintervaller (agingBucket): not-due, 0-30, 31-60, 61-90, 90+.",
    ],
  },
  // ===== END PAYABLES / KREDITORSTYRING =====
];
