import type { CommandSpec } from "./_shared";

export const vatSpecs: CommandSpec[] = [
  {
    key: "vat report",
    usage: "vat report --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
    description: "Bygger en momsrapport for perioden.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "--json/--format json-outputtets feltliste er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå rapportens skema op dér før maskinel parsing.",
    ],
  },
  {
    key: "vat post-eu-service-purchase",
    usage: "vat post-eu-service-purchase --company <path> --input <file.json>",
    description: "Bogfører et EU-servicekøb med reverse charge.",
    allowedFlags: ["--company", "--input"],
    examplePath: "examples/eu-service-purchase.json",
    inputNotes: [
      "transactionDate: YYYY-MM-DD (påkrævet)",
      "text: tekst (påkrævet)",
      "invoiceNo: leverandørfakturaens nummer — slås op til documentId, ELLER angiv documentId: heltal direkte",
      "netAmount: positivt beløb i KRONER (decimal) — grundlaget eksklusive moms",
      "expenseAccountNo: udgiftskonto (påkrævet, fx \"3010\")",
      "paymentAccountNo: valgfri betalingskonto (standard 2000)",
      "Kun EU-leverandører uden for DK; bilagets sender_vat_cvr skal være VIES-valideret",
      "Ved currency != DKK kræves amountForeign, amountDkk og fxRateToDkk (beløb i KRONER)",
    ],
  },
  {
    key: "vat post-representation-purchase",
    usage: "vat post-representation-purchase --company <path> --input <file.json>",
    description: "Bogfører repræsentationsudgift med delvis momsfradrag.",
    allowedFlags: ["--company", "--input"],
    examplePath: "examples/representation-purchase.json",
    inputNotes: [
      "transactionDate: YYYY-MM-DD (påkrævet)",
      "text: tekst (påkrævet)",
      "documentId: heltal — bilags-id (påkrævet, positivt heltal)",
      "netAmount: positivt beløb i KRONER (decimal) — grundlaget eksklusive moms",
      "expenseAccountNo: valgfri udgiftskonto (standard 3070)",
      "paymentAccountNo: valgfri betalingskonto (standard 2000)",
      "Kun 25% af momsen fradrages; resten bogføres som ikke-fradragsberettiget",
      "Ved currency != DKK kræves amountForeign, amountDkk og fxRateToDkk (beløb i KRONER)",
    ],
  },
  {
    key: "vat eu-sales-list",
    usage: "vat eu-sales-list --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
    description: "Bygger EU-salg uden moms-listen (VIES recapitulative statement) for perioden: en liste pr. kunde med EU-momsnummer og samlet værdi af grænseoverskridende B2B-salg uden dansk moms. En selvstændig indberetning ved siden af momsangivelsen.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "Listen dækker udstedte fakturaer med vatTreatment foreign_reverse_charge i perioden (efter udstedelsesdato).",
      "Kun fakturaer med et parsebart EU-momsnummer (ikke dansk) på køberen tæller med; øvrige rapporteres som advarsler.",
      "Read-only: Rentemester producerer listen — du indberetter den selv. Kræver ikke en lukket periode.",
    ],
  },
  {
    key: "vat oss-report",
    usage: "vat oss-report --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
    description: "Bygger en deterministisk OSS-rapport (One Stop Shop, første slice): det samlede grundlag for digitale ydelser solgt til EU-forbrugere bogført med momskoden OSS_EU_CONSUMER. Bevidst smal — ikke en OSS-indberetning til SKAT.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "Rapporten viser kun OSS-forbrugersalgets samlede grundlag — den splitter ikke pr. destinationsland eller momssats.",
      "OSS-salg holdes ude af momsangivelsens standard-rubrikker; OSS-returangivelsen indberetter du selv.",
      "Read-only. Kræver ikke en lukket periode.",
    ],
  },
];

// ===== VAT FILING (#178) =====
export const vatFilingSpecs: CommandSpec[] = [
  {
    key: "vat momsangivelse",
    usage: "vat momsangivelse --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
    description: "Bygger en indberetningsklar momsangivelse (SKAT-rubrikker + momstilsvar) for en lukket momsperiode. Kræver en lukket/indberettet vat_quarter-periode.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "FORUDSÆTNING: --from..--to skal præcist matche en LUKKET (closed) eller INDBERETTET (reported) vat_quarter-periode. Er der ingen sådan periode, afvises kaldet med exit 1 og errors[]: \"VAT period <from>..<to> is not closed: a momsangivelse requires a closed or reported vat_quarter accounting period covering exactly this period — run 'period close' first\".",
      "RETTELSE: luk perioden først med 'rentemester period close --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> --kind vat_quarter' (kræver en actor), og kør derefter momsangivelsen igen med nøjagtigt samme datoer.",
      "--json/--format json-outputtets felter (SKAT-rubrikker, momstilsvar m.m.) er en CLI-only rapport. Det fulde skema er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå formen op dér før maskinel parsing.",
    ],
  },
  {
    key: "vat filing",
    usage: "vat filing --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
    description: "Alias for 'vat momsangivelse': indberetningsklar momsangivelse for en lukket momsperiode.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "FORUDSÆTNING: --from..--to skal præcist matche en LUKKET (closed) eller INDBERETTET (reported) vat_quarter-periode. Er der ingen sådan periode, afvises kaldet med exit 1 og errors[]: \"VAT period <from>..<to> is not closed: a momsangivelse requires a closed or reported vat_quarter accounting period covering exactly this period — run 'period close' first\".",
      "RETTELSE: luk perioden først med 'rentemester period close --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> --kind vat_quarter' (kræver en actor), og kør derefter momsangivelsen igen med nøjagtigt samme datoer.",
      "--json/--format json-outputtets felter (SKAT-rubrikker, momstilsvar m.m.) er en CLI-only rapport. Det fulde skema er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå formen op dér før maskinel parsing.",
    ],
  },
  // ===== END VAT FILING (#178) =====
];
