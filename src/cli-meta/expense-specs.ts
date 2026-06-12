import type { CommandSpec } from "./_shared";

export const expenseSpecs: CommandSpec[] = [
  {
    key: "expense book",
    usage:
      "expense book --company <path> --document-id <n> --bank-transaction-id <n> --expense-account <konto> [--vat-treatment standard|reverse_charge|representation|exempt] [--payment-account <konto>] [--date <YYYY-MM-DD>] [--text <tekst>]",
    description: "Bogfører en leverandørudgift direkte fra bilag + bankpost.",
    allowedFlags: ["--company", "--document-id", "--bank-transaction-id", "--expense-account", "--vat-treatment", "--payment-account", "--date", "--text"],
    inputNotes: [
      "--document-id og --bank-transaction-id binder udgiften til et indlæst bilag og en importeret bankpost (heltal-id'er)",
      "--expense-account: kontonummeret udgiften bogføres på (fx 3000 Software og SaaS)",
      "--vat-treatment styrer momsbehandlingen; udelades den, udledes den af udgiftskontoens default_vat_code:",
      "  standard = dansk købsmoms 25 % løftes af bilaget",
      "  reverse_charge = EU-servicekøb, omvendt betalingspligt (ingen dansk købsmoms på fakturaen)",
      "  representation = repræsentation, kun delvis momsfradrag efter de særlige regler",
      "  exempt = momsfri udgift, intet købsmomsfradrag",
      "  Har kontoen ingen (eller en umappet) default_vat_code, er --vat-treatment påkrævet",
      "--payment-account: betalingskontoen udgiften krediteres på; standard er 2000 (Bank) — sæt den kun, hvis betalingen kom fra en anden konto",
      "--date: bogføringsdato YYYY-MM-DD; udelades den, bruges bankpostens dato",
    ],
  },
];
