import type { CommandSpec } from "./_shared";

// ===== OPENING BALANCE (#179) =====
export const openingBalanceSpecs: CommandSpec[] = [
  {
    key: "opening-balance post",
    usage: "opening-balance post --company <path> --input <file.json>",
    description: "Bogfører virksomhedens primobalance som én balanceret, audited åbningspostering pr. en skæringsdato. Idempotent: præcis én primobalance pr. virksomhed.",
    allowedFlags: ["--company", "--input"],
    examplePath: "examples/opening-balance.post.json",
    exampleHint: "rentemester opening-balance post --example > primobalance.json",
    inputNotes: [
      "cutOverDate: YYYY-MM-DD (skæringsdato)",
      "lines: [{ accountNo, debitAmount | creditAmount, text? }]",
      "debitAmount/creditAmount: beløb i KRONER (decimal, fx 50000.00) — ikke øre",
      "Skal balancere: sum debet == sum kredit (kontrolleres internt med øre-præcision)",
      "note: valgfri tekst",
    ],
  },
  // ===== END OPENING BALANCE (#179) =====
];
