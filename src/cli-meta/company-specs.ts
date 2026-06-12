import type { CommandSpec } from "./_shared";

export const companySpecs: CommandSpec[] = [
  {
    key: "company add",
    usage: "company add [--workspace <dir>] --name <text> [--slug <slug>] [--cvr <DK12345678>] [--address <text>] [--postal-code <text>] [--city <text>] [--payment-terms <0-365>] [--bank-name <text>] [--bank-reg <regnr>] [--bank-account <kontonr>] [--iban <IBAN>] [--fiscal-year-start-month <1-12>] [--fiscal-year-label-strategy end-year|start-year|span]",
    description: "Opretter en ny virksomhed i workspacet (opretter workspacet ved første kørsel). Virksomhedens identitet og betalingsoplysninger registreres her én gang og flyder automatisk med på hver udstedt faktura og dens PDF.",
    allowedFlags: ["--workspace", "--name", "--slug", "--cvr", "--address", "--postal-code", "--city", "--payment-terms", "--bank-name", "--bank-reg", "--bank-account", "--iban", "--fiscal-year-start-month", "--fiscal-year-label-strategy"],
    inputNotes: [
      "IKKE idempotent: et gentaget kald med samme --name/--slug overskriver ALDRIG en eksisterende virksomhed. Findes der allerede en virksomhed på <workspace>/<slug>/, afvises kaldet med 'a company already exists at <sti>'; et slug der allerede står i workspace-manifestet afvises ligeledes.",
      "Slug'et udledes fra --name når --slug udelades. For at oprette endnu en virksomhed med samme navn skal et nyt, unikt --slug angives eksplicit.",
    ],
  },
  {
    key: "company list",
    usage: "company list [--workspace <dir>]",
    description: "Lister virksomheder i workspacet.",
    allowedFlags: ["--workspace"],
  },
  {
    key: "company sync-cvr",
    usage: "company sync-cvr --company <slug|path>",
    description: "Henter virksomhedens stamdata fra CVR-registret og opdaterer navn, adresse, branche, virksomhedsform og status. Regnskabsåret røres aldrig — et afvigende regnskabsår rapporteres kun.",
    allowedFlags: ["--company"],
    inputNotes: [
      "Kræver miljøvariablerne CVR_USERNAME og CVR_PASSWORD (opret adgang på virk.dk)",
      "Virksomheden skal have et CVR-nummer registreret",
      "CVR-data er et snapshot — det caches lokalt og læses aldrig live under bogføring",
    ],
  },
  {
    // #267: `init` (and `init --help`) point owners here to set the bank/
    // payment details they skipped at onboarding. The command must therefore
    // be discoverable and self-documenting — its --help, flags and --example
    // are part of that promise.
    key: "company set-profile",
    usage: "company set-profile --company <slug|path> [--name <text>] [--cvr <DK12345678>] [--address <text>] [--postal-code <text>] [--city <text>] [--payment-terms <0-365>] [--vat-period month|quarter|half-year] [--bank-name <text>] [--bank-reg <regnr>] [--bank-account <kontonr>] [--iban <IBAN>]",
    description: "Retter virksomhedens egen profil efter init: navn, CVR, adresse, betalingsfrist, momsperiode og betalingsoplysninger (bankkonto/IBAN). Hver efterfølgende udstedt faktura og dens PDF arver de nye værdier automatisk — du indtaster aldrig din egen stamdata på en faktura.",
    allowedFlags: ["--company", "--name", "--cvr", "--address", "--postal-code", "--city", "--payment-terms", "--vat-period", "--bank-name", "--bank-reg", "--bank-account", "--iban"],
    examplePath: "examples/company-set-profile.txt",
    exampleHint: "rentemester company set-profile --example",
    exampleNote: "Eksemplet er en kommandolinje-skabelon — udskift <sti-eller-slug> og bankoplysningerne med dine egne.",
    inputNotes: [
      "Kun de flag du faktisk angiver, ændres — de øvrige profilfelter beholder deres nuværende værdi.",
      "Angav du ingen bankkonto ved 'init', så sæt --bank-name/--bank-reg/--bank-account (og evt. --iban) her: uden dem får en udstedt fakturas PDF INGEN BETALING-blok, og kunden kan ikke se hvor pengene skal hen.",
      "Bankkontoen er append-only: den oprettes ved første kald med bankoplysninger. Et senere kald opretter ikke en ny konto — opdatér i stedet kontoen direkte hvis oplysningerne ændrer sig.",
      "--payment-terms er standard betalingsfrist i dage (udstedelsesdato → forfaldsdato), heltal 0-365.",
      "--vat-period ændrer virksomhedens momsperiode: month (måneds-moms), quarter (kvartals-moms) eller half-year (halvårs-moms). Vælg den periode du er registreret for hos SKAT — momsperioder, -frister og momsangivelsen følger dette valg overalt (dashboard, cockpit, 'period close').",
    ],
  },
  {
    key: "company profile",
    usage: "company profile --company <slug|path>",
    description: "Viser virksomhedens nuværende profil: navn, CVR, adresse, standard betalingsfrist og momsperiode. Ren læsning — ændrer intet.",
    allowedFlags: ["--company"],
  },
];
