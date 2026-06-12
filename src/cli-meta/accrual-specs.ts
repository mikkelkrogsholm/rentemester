import type { CommandSpec } from "./_shared";

// ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
export const accrualSpecs: CommandSpec[] = [
  {
    key: "accrual register",
    usage:
      "accrual register --company <path> --type prepaid_expense|accrued_expense|deferred_revenue --description <text> --amount <n> --periods <n> --first-date <YYYY-MM-DD> --result-account <konto> [--registration-date <YYYY-MM-DD>] [--period-step-months <n>] [--balance-account <konto>] [--settlement-account <konto>] [--document-id <n>] [--note <text>]",
    description:
      "Registrerer en periodeafgrænsningspost: bogfører den balancerede registreringspostering, der parkerer beløbet på en balancekonto, og gemmer hovedet, så hver senere periode kan indtægts-/omkostningsføres mod en deterministisk plan.",
    allowedFlags: [
      "--company", "--type", "--description", "--amount", "--periods", "--first-date",
      "--result-account", "--registration-date", "--period-step-months", "--balance-account",
      "--settlement-account", "--document-id", "--note",
    ],
    inputNotes: [
      "--type: prepaid_expense (forudbetalt omkostning — aktiv), accrued_expense (skyldig omkostning — passiv), deferred_revenue (forudbetalt indtægt — passiv).",
      "--amount: hele beløbet i KRONER (decimal, fx 12000.00) — ikke øre. Det fordeles ligeligt over --periods perioder; sidste periode bærer øre-resten, så planen summer præcist.",
      "--periods: antal perioder beløbet periodiseres over (positivt heltal).",
      "--first-date: bogføringsdato for første periode (YYYY-MM-DD). --period-step-months sætter antal måneder mellem perioderne (standard 1).",
      "--result-account: resultatkontoen hver periode føres på — en udgiftskonto for prepaid_expense/accrued_expense, en indtægtskonto for deferred_revenue.",
      "--balance-account: balancekontoen beløbet parkeres på; defaulter pr. type (1300 / 7300 / 7310). --settlement-account: betalingskonto på modposten (standard 2000 Bank).",
      "--registration-date: bogføringsdato for registreringsposteringen (standard: --first-date).",
      "--document-id: heltal — bilags-id. Påkrævet hvis registreringen rammer en udgifts-/indtægtskonto (accrued_expense og deferred_revenue gør altid).",
    ],
  },
  {
    key: "accrual recognize",
    usage: "accrual recognize --company <path> --accrual-id <n> --period <n> [--date <YYYY-MM-DD>] [--settlement-account <konto>]",
    description:
      "Indtægts-/omkostningsfører én periode af en periodeafgrænsningspost: bogfører den balancerede postering, der flytter periodens andel mellem balancekontoen og resultatkontoen.",
    allowedFlags: ["--company", "--accrual-id", "--period", "--date", "--settlement-account"],
    inputNotes: [
      "--accrual-id: heltal — id på en post registreret med 'accrual register'.",
      "--period <n>: periodens INDEKS i posten plan, IKKE en kalendermåned. 1 = første periode, op til --periods. Hver periode kan kun bogføres én gang.",
      "--date: bogføringsdato YYYY-MM-DD; udelades den, bruges planens dato for perioden.",
      "--settlement-account: kun brugt af accrued_expense (afviklingen krediterer betalingskontoen; standard 2000).",
      "Beløbet beregnes af den deterministiske plan — du angiver det aldrig selv.",
    ],
  },
  { key: "accrual register-report", usage: "accrual register-report --company <path>", description: "Viser registret af periodeafgrænsningsposter med bogførte perioder, periodiseret beløb og resterende balanceeksponering.", allowedFlags: ["--company"] },
  // ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
];
