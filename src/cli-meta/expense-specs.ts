import type { CommandSpec } from "./_shared";

const bookingFlags = ["--company", "--document-id", "--bank-transaction-id", "--expense-account", "--vat-treatment", "--payment-account", "--date", "--text"];

export const expenseSpecs: CommandSpec[] = [
  {
    key: "expense vat-preflight",
    usage: "expense vat-preflight --company <path> --document-id <n> [--apply yes]",
    description: "Viser købsmoms-preflight uden sideeffekter; --apply yes henter kun nødvendig EU-VAT-evidens.",
    allowedFlags: ["--company", "--document-id", "--apply"],
    inputNotes: ["Uden --apply er kommandoen en ren dry-run: region, krævet validering, cache-friskhed og provider-kald vises.", "Kun den eksakte form --apply yes må skrive; den kræver actor-attribution og gemmer kun sikker, resumérbar evidens."],
  },
  {
    key: "expense dry-run",
    usage: "expense dry-run --company <path> --document-id <n> --bank-transaction-id <n> --expense-account <konto> [--vat-treatment <behandling>] [--payment-account <konto>] [--date <YYYY-MM-DD>] [--text <tekst>]",
    description: "Viser udgiftens bogføringslinjer efter kontrol af bilag, bankpost og momsevidens på en midlertidig kopi.",
    allowedFlags: bookingFlags,
  },
  {
    key: "expense book",
    usage:
      "expense book --company <path> --document-id <n> --bank-transaction-id <n> --expense-account <konto> [--vat-treatment standard|reverse_charge|representation|exempt|non_deductible] [--payment-account <konto>] [--date <YYYY-MM-DD>] [--text <tekst>]",
    description: "Bogfører en leverandørudgift direkte fra bilag + bankpost.",
    allowedFlags: bookingFlags,
    inputNotes: [
      "--document-id og --bank-transaction-id binder udgiften til et indlæst bilag og en importeret bankpost (heltal-id'er)",
      "--expense-account: kontonummeret udgiften bogføres på (fx 3000 Software og SaaS)",
      "--vat-treatment styrer momsbehandlingen; udelades den, udledes den af udgiftskontoens default_vat_code:",
      "  standard = dansk købsmoms 25 % løftes af bilaget",
      "  reverse_charge = udenlandsk servicekøb (EU eller ikke-EU), omvendt betalingspligt (ingen dansk moms på fakturaen; leverandøridentiteten vælger korrekt behandling)",
      "  representation = repræsentation, kun delvis momsfradrag efter de særlige regler",
      "  exempt = momsfri udgift, intet købsmomsfradrag",
      "  non_deductible = moms uden fradragsret (fx udenlandsk lokal skat eller bilag hos en ikke-momsregistreret virksomhed): hele bilaget bogføres brutto på udgiftskontoen, ingen 4000 Købsmoms-linje, momsen indgår i kostprisen. Kan vælges eksplicit også i et momsregistreret selskab.",
      "  Har kontoen ingen (eller en umappet) default_vat_code, er --vat-treatment påkrævet",
      "--payment-account: betalingskontoen udgiften krediteres på; standard er 2000 (Bank) — sæt den kun, hvis betalingen kom fra en anden konto",
      "Ved bilag i fremmed valuta og en bankpost i DKK bruges bankpostens DKK-beløb som afregning; mangler importeret fx_rate_to_dkk, udledes den deterministisk af bilagets bruttobeløb og DKK-afregningen.",
      "--date: bogføringsdato YYYY-MM-DD; udelades den, bruges bankpostens dato",
    ],
  },
];
