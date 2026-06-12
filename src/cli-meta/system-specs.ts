import type { CommandSpec } from "./_shared";

export const initSpec: CommandSpec[] = [
  {
    key: "init",
    usage: "init --company <path> [--workspace <dir>] [--name <text>] [--cvr <DK12345678>] [--address <text>] [--postal-code <text>] [--city <text>] [--payment-terms <0-365>] [--vat-period month|quarter|half-year] [--bank-name <text>] [--bank-reg <regnr>] [--bank-account <kontonr>] [--iban <IBAN>] [--fiscal-year-start-month <1-12>] [--fiscal-year-label-strategy end-year|start-year|span]",
    description: "Initialiserer en virksomhed og opretter standardkontoplan. Virksomhedens egen identitet (navn, adresse, CVR) og betalingsoplysninger (bankkonto/IBAN, betalingsfrist) registreres her én gang og flyder automatisk med på hver udstedt faktura og dens PDF.",
    allowedFlags: ["--company", "--workspace", "--name", "--cvr", "--address", "--postal-code", "--city", "--payment-terms", "--vat-period", "--bank-name", "--bank-reg", "--bank-account", "--iban", "--fiscal-year-start-month", "--fiscal-year-label-strategy"],
    examplePath: "examples/init-vat-period.txt",
    exampleHint: "rentemester init --example",
    exampleNote: "Eksemplet er en kommandolinje-skabelon — udskift <sti> med din egen virksomhedsmappe og vælg den momsperiode du er registreret for hos SKAT.",
    inputNotes: [
      "Ligger virksomhedsmappen i et workspace (via --workspace eller RENTEMESTER_WORKSPACE), registreres virksomheden også i workspacet, så Cockpittet kan se den.",
      "--vat-period sætter virksomhedens momsperiode: month (måneds-moms), quarter (kvartals-moms) eller half-year (halvårs-moms). Standard er quarter. Vælg den periode du er registreret for hos SKAT — momsperioder og -frister følger dette valg.",
      "Virksomhedsprofilen kan rettes senere med 'company set-profile' — du behøver aldrig at indtaste din egen stamdata på en faktura igen.",
    ],
  },
];

export const serveSpec: CommandSpec[] = [
  {
    key: "serve",
    usage: "serve [--workspace <dir>] [--host <addr>] [--port <n>]",
    description: "Starter cockpit-backenden: en lokal JSON-API over workspacet — overblik, workspace-styring og menneske-styrede bogføringshandlinger (fakturering, bank-CSV-import, bilagsindlæsning).",
    allowedFlags: ["--workspace", "--host", "--port"],
    inputNotes: [
      "Bind-adressen er konfigurations-styret: standard 127.0.0.1 (kun localhost)",
      "Miljøvariabler: RENTEMESTER_APP_HOST, RENTEMESTER_APP_PORT, RENTEMESTER_WORKSPACE",
      "Cockpittet kan udstede fakturaer, importere bankudtog og indlæse bilag — hver skrivehandling går gennem backup-låsen og actor-attribuering, ligesom agent/CLI-stien",
    ],
  },
];

export const systemSpecs: CommandSpec[] = [
  { key: "system healthcheck", usage: "system healthcheck --company <slug|path>", description: "Tjekker at virksomhedsmappen og kernefiler findes.", allowedFlags: ["--company"] },
  { key: "system backup", usage: "system backup --company <path> [--at <ISO-8601>] [--sign-with-ed25519] [--archive]", description: "Opretter en revisionsklar backup. Med --sign-with-ed25519 tilføjes en asymmetrisk signatur som 3.-part kan verificere uafhængigt. Med --archive pakkes backuppen straks til én .tar-fil klar til off-site placering.", allowedFlags: ["--company", "--at", "--sign-with-ed25519", "--archive"] },
  { key: "system backup-status", usage: "system backup-status --company <path> [--as-of <ISO-8601>]", description: "Viser om backup-pligten er opfyldt.", allowedFlags: ["--company", "--as-of"] },
  {
    key: "system restore-backup",
    usage: "system restore-backup --backup-dir <dir-eller-.tar> --target-company <path> --confirm yes [--verify-key <path>] [--public-key <path>]",
    description:
      "DESTRUKTIV: gendanner en backup til --target-company. Sletter ingen filer på source, men OVERSKRIVER eksisterende filer i --target-company. Kræver --confirm yes — uden den afvises kommandoen uden at røre filsystemet. --backup-dir kan pege på enten en backup-mappe eller et .tar-arkiv (kræver typisk --verify-key for et arkiv).",
    allowedFlags: ["--backup-dir", "--target-company", "--confirm", "--verify-key", "--public-key"],
    inputNotes: [
      "--confirm yes er påkrævet: restore kan overskrive filer i --target-company",
      "Restore rører aldrig backup-kilden — kun --target-company skrives",
      "--verify-key peger på den symmetriske HMAC-nøgle, der godtgør backuppens ægthed (manifestets HMAC-tag). Kræves typisk for et .tar-arkiv; for en backup-mappe udledes nøglen ellers fra virksomhedens backups-mappe.",
      "--public-key peger på den asymmetriske ed25519-offentlige nøgle, der verificerer backuppens ed25519-signatur — den signatur 'system backup --sign-with-ed25519' tilføjer, så en uafhængig 3.-part kan bekræfte ægtheden uden at kende HMAC-nøglen. Den adskiller sig fra --verify-key: --verify-key er den symmetriske HMAC-nøgle, --public-key den asymmetriske ed25519-nøgle. Se docs/backup-security.md.",
      "MCP-ækvivalenten system_restore_backup kræver confirm:true + confirmText='RESTORE <targetCompany>'",
    ],
  },
  { key: "system backup-archive", usage: "system backup-archive --company <path> [--backup-id <id>] [--out <file.tar>]", description: "Pakker en eksisterende backup til én deterministisk .tar-fil (+ .sha256-sidecar) klar til at flytte off-site. Uden --backup-id pakkes den nyeste backup.", allowedFlags: ["--company", "--backup-id", "--out"] },
  { key: "system backup-governance", usage: "system backup-governance --company <path> [--as-of <ISO-8601>]", description: "Viser den samlede backup-status: forfald, bogførings-lås, destinationer og om seneste backup er placeret sikkert i EU/EØS.", allowedFlags: ["--company", "--as-of"] },
  { key: "system backup-destinations", usage: "system backup-destinations --company <path>", description: "Lister konfigurerede backup-destinationer med deres EU/EØS- og it-sikkerheds-attestering.", allowedFlags: ["--company"] },
  {
    key: "system backup-add-destination",
    usage: "system backup-add-destination --company <path> --label <text> --kind local-folder|dropbox|google-drive|ssh|other --location <path|uri> --region-eu true|false --attested-by <actor> [--region-country <kode>] [--region-note <text>] [--non-related true|false] [--it-security true|false] [--it-security-note <text>] [--at <ISO-8601>]",
    description: "Tilføjer en backup-destination. Du attesterer som menneske om destinationen ligger på en server i EU/EØS, jf. BEK 205/2024 § 4, stk. 2 — Rentemester kan ikke selv vide det.",
    allowedFlags: ["--company", "--label", "--kind", "--location", "--region-eu", "--attested-by", "--region-country", "--region-note", "--non-related", "--it-security", "--it-security-note", "--at"],
  },
  { key: "system backup-remove-destination", usage: "system backup-remove-destination --company <path> --id <dest-id>", description: "Fjerner en konfigureret backup-destination.", allowedFlags: ["--company", "--id"] },
  {
    key: "system backup-place",
    usage: "system backup-place --company <path> --archive-file <file.tar> --destination <dest-id> [--actor-kind human|agent] [--at <ISO-8601>] [--note <text>]",
    description: "Kopierer et backup-arkiv til en destination med en lokal/synkroniseret mappe (fx en Dropbox- eller Google Drive-desktopmappe) og verificerer kopien med sha256.",
    allowedFlags: ["--company", "--archive-file", "--destination", "--actor-kind", "--at", "--note"],
  },
  {
    key: "system backup-confirm-placement",
    usage: "system backup-confirm-placement --company <path> --destination <dest-id> --backup-id <id> --archive-sha256 <hex> [--archive-size <bytes>] [--actor-kind human|agent] [--at <ISO-8601>] [--note <text>]",
    description: "Registrerer en backup-placering foretaget uden for Rentemester — fx en agent der har pushet arkivet til Dropbox/Drive/SSH med egne værktøjer. Verificeres med sha256 hvis destinationen er læsbar.",
    allowedFlags: ["--company", "--destination", "--backup-id", "--archive-sha256", "--archive-size", "--actor-kind", "--at", "--note"],
  },
  {
    key: "system backup-lock",
    usage: "system backup-lock --company <path> [--enforce true|false] [--grace-days <n>] [--at <ISO-8601>]",
    description: "Konfigurerer den frivillige bogførings-lås. Når den er slået til, blokeres ny bogføring hvis den ugentlige backup (BEK 205/2024 § 4) er forsømt ud over en eventuel grace-periode. system-, backup- og restore-kommandoer blokeres aldrig.",
    allowedFlags: ["--company", "--enforce", "--grace-days", "--at"],
  },
  { key: "system backup-guide", usage: "system backup-guide --company <path> --out <file.html> [--as-of <ISO-8601>]", description: "Genererer en HTML-side der forklarer backup-reglerne (bogføringsloven § 12/§ 15, BEK 205/2024 § 4) og viser virksomhedens aktuelle backup-status.", allowedFlags: ["--company", "--out", "--as-of"] },
  { key: "system export-public-key", usage: "system export-public-key --company <path> --out <file>", description: "Eksporterer virksomhedens ed25519 backup-public-key til en fil (deles med revisor/myndighed).", allowedFlags: ["--company", "--out"] },
  { key: "system verify-backup-signature", usage: "system verify-backup-signature --backup-dir <dir> [--public-key <path>] [--verify-key <path>]", description: "Verificerer signaturen på en backup uden at restore. Med --public-key kan en 3.-part verificere uden source-adgang.", allowedFlags: ["--backup-dir", "--public-key", "--verify-key"] },
  { key: "system rotate-backup-keypair", usage: 'system rotate-backup-keypair --company <path> --reason "<text>" [--at <ISO-timestamp>]', description: "Roterer Ed25519 backup-nøglepar: arkiverer den gamle og opretter en ny. Audit-logges. Ældre backups verificeres stadig med den arkiverede public-key.", allowedFlags: ["--company", "--reason", "--at"] },
  { key: "system export-authority", usage: "system export-authority --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> --out <dir> [--requested-at <ISO-8601>] [--requester <name>]", description: "Eksporterer materiale til myndighedsudlevering.", allowedFlags: ["--company", "--from", "--to", "--out", "--requested-at", "--requester"] },
  { key: "system export-accountant", usage: "system export-accountant --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> --out <dir> [--requested-at <ISO-8601>] [--requester <name>]", description: "Eksporterer en deterministisk lokal håndoff-pakke til bogholder eller revisor.", allowedFlags: ["--company", "--from", "--to", "--out", "--requested-at", "--requester"] },
  { key: "system export-saft", usage: "system export-saft --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> --out <dir> [--generated-at <ISO-8601>]", description: "Eksporterer første deterministiske SAF-T-slice (kontoplan, journal og salgsfakturaer).", allowedFlags: ["--company", "--from", "--to", "--out", "--generated-at"] },
  { key: "audit verify", usage: "audit verify --company <path>", description: "Verificerer audit-kæde og bogføringsintegritet.", allowedFlags: ["--company"] },
  { key: "accounts list", usage: "accounts list --company <path>", description: "Lister kontoplanen.", allowedFlags: ["--company"] },
];
