import type { CommandSpec } from "./_shared";

export const journalSpecs: CommandSpec[] = [
  {
    key: "journal post",
    usage: "journal post --company <path> --input <file.json>",
    description: "Bogfører en manuel finanspostering. Brug 'journal dry-run' først for en ikke-bindende forhåndsvisning.",
    allowedFlags: ["--company", "--input"],
    examplePath: "examples/journal-entry.expense.json",
    inputNotes: [
      "transactionDate: YYYY-MM-DD (påkrævet)",
      "text: tekst (påkrævet)",
      "lines: mindst 2 linjer; hver linje { accountNo, debitAmount | creditAmount, vatCode?, text? }",
      "debitAmount/creditAmount: positivt beløb i KRONER (decimal, fx 1250.50) — ikke øre",
      "Hver linje har enten debitAmount eller creditAmount, aldrig begge; sum debet == sum kredit",
      "documentId: heltal — PÅKRÆVET når en linje rammer en udgifts- eller indtægtskonto (bilagsbevis)",
      'currency: valgfri 3-bogstavs ISO (standard "DKK")',
      "Ved currency != DKK kræves desuden amountForeign, amountDkk og fxRateToDkk (beløb i KRONER)",
    ],
  },
  {
    key: "journal dry-run",
    usage: "journal dry-run --company <path> --input <file.json>",
    description: "Forhåndsviser en finanspostering uden at bogføre den (ikke-bindende).",
    allowedFlags: ["--company", "--input"],
    examplePath: "examples/journal-entry.expense.json",
    inputNotes: [
      "Samme JSON-payload som 'journal post'.",
      "Skriver intet i den append-only kæde: intet journalnummer forbruges, intet bilag bogføres.",
      "Svaret rummer entryNo og entryHash posteringen ville få, samt accountEffects (saldo før/efter).",
      "Resultatet er ikke-bindende — entryNo og saldi kan ændre sig hvis en anden postering bogføres inden 'journal post'.",
    ],
  },
  {
    key: "journal reverse",
    usage:
      "journal reverse --company <path> (--entry-id <n> | --entry-no <no> | --match-text <text> [--match-date <YYYY-MM-DD>] [--match-document-id <n>]) --date <YYYY-MM-DD> --reason <text>",
    description: "Tilbagefører en bogført finanspostering.",
    allowedFlags: ["--company", "--entry-id", "--entry-no", "--match-text", "--match-date", "--match-document-id", "--date", "--reason"],
  },
  { key: "journal list", usage: "journal list --company <path>", description: "Lister finansposteringer.", allowedFlags: ["--company"] },
];
