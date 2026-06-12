import type { CommandSpec } from "./_shared";

// ===== BUDGET + LIQUIDITY FORECAST =====
export const budgetSpecs: CommandSpec[] = [
  {
    key: "budget set",
    usage: "budget set --company <path> --account <kontonr> --period <YYYY-MM> --amount <kroner> [--notes <text>]",
    description:
      "Sætter et budget for én konto i én kalendermåned. Budgetlinjer er append-only: hvert kald tilføjer en ny revision, og den seneste revision er det gældende budget — hele ændringshistorikken bevares til revision.",
    allowedFlags: ["--company", "--account", "--period", "--amount", "--notes"],
    inputNotes: [
      "--account: kontonummer der skal findes i kontoplanen ('accounts list').",
      "--period: kalendermåned i YYYY-MM-format (fx 2026-06).",
      "--amount: budgetbeløb i KRONER (decimal) i kontoens naturlige fortegn — aldrig negativt.",
    ],
  },
  {
    key: "budget list",
    usage: "budget list --company <path> [--period <YYYY-MM>] [--account <kontonr>]",
    description: "Lister de gældende (seneste-revision) budgetlinjer, valgfrit filtreret på periode eller konto.",
    allowedFlags: ["--company", "--period", "--account"],
  },
  {
    key: "budget vs-actual",
    usage: "budget vs-actual --company <path> --from <YYYY-MM> --to <YYYY-MM>",
    description:
      "Sammenligner budget mod faktisk bogføring pr. konto pr. måned i perioden. Dækker resultatkonti (indtægt/udgift) med bevægelse plus enhver konto der bærer en budgetlinje. Afvigelse = budget − faktisk for udgift, faktisk − budget for indtægt.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "--from / --to: kalendermåneder i YYYY-MM-format; --from skal være ≤ --to.",
      "--json/--format json-outputtets feltliste er dokumenteret i docs/cli-contract.md før maskinel parsing.",
    ],
  },
  {
    key: "budget forecast",
    usage: "budget forecast --company <path> --start <YYYY-MM-DD> --months <n>",
    description:
      "Fremskriver banksaldoen måned for måned: primosaldo (bogført banksaldo) + åbne fakturaer der forfalder + planlagte gentagne fakturaer + budgetterede omkostninger = projiceret ultimosaldo pr. periode. Rent deterministisk aritmetik — ingen statistisk model.",
    allowedFlags: ["--company", "--start", "--months"],
    inputNotes: [
      "--start: første dag i den første måned der fremskrives (YYYY-MM-DD).",
      "--months: antal sammenhængende månedsperioder (1-18).",
      "Prognosen er kun så komplet som det vedligeholdte budget; uplanlagt forbrug, moms-afregning og løn er uden for scope.",
    ],
  },
  // ===== END BUDGET + LIQUIDITY FORECAST =====
];
