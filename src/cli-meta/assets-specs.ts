import type { CommandSpec } from "./_shared";

// ===== FIXED ASSETS (#124, #125) =====
export const assetSpecs: CommandSpec[] = [
  {
    key: "asset register",
    usage:
      "asset register --company <path> --name <text> --category <text> --acquisition-date <YYYY-MM-DD> --cost <n> --useful-life-months <n> --document-id <n> [--asset-account <konto>] [--depreciation-account <konto>] [--accumulated-account <konto>] [--note <text>]",
    description: "Registrerer et aktiv til afskrivning over tid med en lineær afskrivningsplan.",
    allowedFlags: ["--company", "--name", "--category", "--acquisition-date", "--cost", "--useful-life-months", "--document-id", "--asset-account", "--depreciation-account", "--accumulated-account", "--note"],
    inputNotes: [
      "--cost: anskaffelsessummen i KRONER (decimal, fx 12000.00) — ikke øre. Skal være positiv.",
      "--category: fri tekst (fx \"IT-udstyr\", \"Inventar\", \"Maskiner\") — ikke en fast liste. Bruges kun til gruppering i aktivregistret.",
      "--useful-life-months: brugstiden i hele måneder (positivt heltal). Costen fordeles lineært over så mange afskrivningsperioder.",
      "--acquisition-date: anskaffelsesdato YYYY-MM-DD.",
      "--document-id: heltal — bilags-id der dokumenterer anskaffelsen (påkrævet bevis).",
      "Kontoflag er valgfri og defaulter: --asset-account 5800, --accumulated-account 5810, --depreciation-account 5820. Angiv kun et flag for at afvige fra standardkontoplanen.",
    ],
  },
  {
    key: "asset depreciate",
    usage: "asset depreciate --company <path> --asset-id <n> --period <n> --date <YYYY-MM-DD>",
    description: "Bogfører en periodes afskrivning for et aktiv (debet afskrivninger, kredit akkumulerede afskrivninger).",
    allowedFlags: ["--company", "--asset-id", "--period", "--date"],
    inputNotes: [
      "--asset-id: heltal — id på et aktiv registreret med 'asset register'.",
      "--period <n>: afskrivningsperiodens INDEKS i aktivets plan, IKKE en kalendermåned. 1 = første periode, op til --useful-life-months. Hver periode kan kun bogføres én gang.",
      "--date: bogføringsdato YYYY-MM-DD for denne periodes afskrivning.",
      "Beløbet beregnes af den lineære plan (cost / useful-life-months) — du angiver det aldrig selv.",
    ],
  },
  {
    key: "asset write-off",
    usage:
      "asset write-off --company <path> --name <text> --category <text> --acquisition-date <YYYY-MM-DD> --cost <n> --document-id <n> --expense-account <konto> --date <YYYY-MM-DD> --threshold-source <text> --confirm yes [--payment-account <konto>] [--note <text>]",
    description: "Bogfører straksafskrivning af et mindre aktiv. Kræver --confirm yes og kildehenvisning til reglen; bruger/revisor ejer den skattemæssige vurdering.",
    allowedFlags: ["--company", "--name", "--category", "--acquisition-date", "--cost", "--document-id", "--expense-account", "--date", "--threshold-source", "--confirm", "--payment-account", "--note"],
    inputNotes: [
      "--confirm yes er PÅKRÆVET: straksafskrivning er en skattemæssig vurdering, som du/din revisor selv tager ansvaret for — kun den ordrette værdi 'yes' bekræfter; alt andet (også at udelade flaget) blokerer kommandoen.",
      "--threshold-source er PÅKRÆVET og skal indeholde en konkret kildehenvisning til den regel/beløbsgrænse, du anvender (fx 'AL § 6, stk. 1, nr. 2' eller en SKAT-vejledning) — Rentemester gemmer henvisningen, men afgør ikke skattereglen for dig.",
      "Beløbsgrænsen Rentemester bruger som arbejdsgang-værn er vejledende (ca. 33.100 kr.); overskrider --cost den, blokeres straksafskrivningen, og en undtagelse lægges i kø til manuel vurdering.",
      "--document-id skal pege på et indlæst købsbilag; mangler bilaget, blokeres write-off'en, og en undtagelse lægges i kø.",
      "--expense-account: kontoen omkostningen bogføres på. --payment-account: betalingskontoen (standard 2000 Bank). --date: bogføringsdato YYYY-MM-DD.",
    ],
  },
  { key: "asset register-report", usage: "asset register-report --company <path>", description: "Viser aktivregister med akkumulerede afskrivninger og bogført værdi.", allowedFlags: ["--company"] },
  // ===== END FIXED ASSETS (#124, #125) =====
];
