import type { CommandSpec } from "./_shared";

export const periodSpecs: CommandSpec[] = [
  {
    key: "period close",
    usage: "period close --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--kind vat_quarter|fiscal_year|custom] [--status closed|reported] [--reference <text>]",
    description: "Lukker eller markerer en regnskabsperiode. En lukket periode blokerer ny bogføring med transaktionsdato i perioden — og er en forudsætning for 'vat momsangivelse' og 'report annual'.",
    allowedFlags: ["--company", "--from", "--to", "--kind", "--status", "--reference"],
    inputNotes: [
      "--from / --to afgrænser perioden (begge YYYY-MM-DD, inklusive). Perioder af samme --kind må ikke overlappe.",
      "--kind: vat_quarter (momsperiode — kræves af 'vat momsangivelse'), fiscal_year (regnskabsår — kræves af 'report annual'), custom. Standard: vat_quarter.",
      "--status: 'closed' = perioden er afsluttet og bogføringen låst; 'reported' = derudover indberettet til myndigheden (SKAT/Erhvervsstyrelsen). Standard: closed.",
      "Begge statusser låser bogføringen lige hårdt — forskellen er kun om indberetning er sket. Vælg 'reported' når du allerede har indsendt; ellers 'closed'.",
      "--reference: valgfri fri tekst der gemmes på perioden (fx kvittering/journalnummer fra indberetningen).",
      "En for tidligt lukket periode kan åbnes igen med 'period reopen' — en kontrolleret, fuldt revisionssporet handling. En 'reported'-periode (indberettet til myndigheden) kan dog ikke åbnes igen.",
    ],
  },
  {
    key: "period reopen",
    usage: "period reopen --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--kind vat_quarter|fiscal_year|custom] --reason <text>",
    description: "Åbner en lukket regnskabsperiode igen via en kontrolleret, fuldt revisionssporet handling. Efter genåbning kan der igen bogføres med transaktionsdato i perioden — luk den igen med 'period close', når rettelsen er bogført.",
    allowedFlags: ["--company", "--from", "--to", "--kind", "--reason"],
    inputNotes: [
      "--from / --to / --kind udpeger den lukkede periode der skal åbnes (samme værdier som ved 'period close').",
      "--reason: PÅKRÆVET fri tekst der begrunder genåbningen. Begrundelsen gemmes ordret i den append-only audit-log sammen med aktør og tidsstempel.",
      "Genåbningen ændrer ALDRIG den oprindelige periode-række — den tilføjer en ny audit-post. Hele luk/genåbn-historikken bevares og kan revideres.",
      "Kræver en actor (--actor <user:...|agent:...|system:...> eller USER/LOGNAME/OPENCLAW_AGENT) så genåbningen er entydigt tilskrivbar.",
      "En 'reported'-periode (allerede indberettet til SKAT/Erhvervsstyrelsen) kan IKKE åbnes igen — ret i stedet via en ny postering i en åben periode.",
    ],
  },
  { key: "retention status", usage: "retention status --company <path> [--as-of <YYYY-MM-DD>]", description: "Viser opbevaringsfrister og udløbet materiale.", allowedFlags: ["--company", "--as-of"] },
];
