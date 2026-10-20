# Backup til en konfigureret mappe i Rust

Rust-serveren kan danne en signeret backup af hele workspacet, placere den i
én operatørkonfigureret mappe og genlæse de præcise bytes. Web, CLI og MCP
bruger samme rettigheder, operation-ID og kvittering. Gendannelse og uafhængig
verifikation følger fortsat [serverens backupkontrakt](rust-server.md).

## Operatørens opsætning

Tilføj `backupDestinations` til serverens eksisterende JSON-konfiguration.
`backupSigningKeyFile` skal også være konfigureret som beskrevet i
servervejledningen. Et eksempel på én destination:

```json
{
  "backupDestinations": [{
    "id": "company-backup",
    "label": "Bogføringsbackup",
    "kind": "local-folder",
    "directory": "/var/lib/rentemester-backup",
    "nonRelatedParty": false,
    "inEeaOrEu": false,
    "country": "DK",
    "meetsRecognisedStandards": false,
    "attestedBy": "Operator",
    "attestedAt": "2026-10-21T00:00:00Z"
  }]
}
```

Eksemplet er kun destinationsdelen af konfigurationen. Der kan være højst
16 destinationer med unikke ID'er. ID'er består af 8–64 små ASCII-bogstaver,
cifre eller bindestreger. Datoen er RFC3339, landet to store ASCII-bogstaver.
Attestationerne er operatørens erklæringer; vælg kun sande værdier på et
selvstændigt dokumenteret grundlag. Produktet verificerer dem ikke.

Mappen skal allerede eksistere, have mode `0700` og tilhøre serverens UID.
Hele stien kontrolleres med samme beskyttede filsystemkontrakt som workspacet;
usikre forældre, symlinks og uacceptable ACL'er afvises. Den absolutte sti skal
ligge uden for workspacet og må heller ikke være dets overordnede mappe.
Serveren opretter eller retter ikke mappens rettigheder. I en container skal
mappen monteres særskilt og ejes af containerens serverbruger.

`kind` kan være `local-folder`, `dropbox` eller `google-drive`. Alle tre betyder
her **kopiering til en lokal eller synkroniseret mappe**. Et udbydernavn beviser
ikke, at en ekstern synkronisering er gennemført. Svarene indeholder altid
`transfer: "local-directory-copy"`, `remoteSyncVerified: false` og
`attestationIndependentlyVerified: false`. Filsystemstien udleveres ikke til
klienterne. Konfiguration og signeringsnøgle følger ikke med som aktiv
autoritet ved gendannelse.

## Brugerforløbet

En aktuelt autentificeret menneskelig bruger skal have `WorkspaceManage` og
`CompanyAdmin` for **alle registrerede virksomheder**, inklusive arkiverede.
En servicekonto kan ikke bruge disse operationer.

I web-UI'ets **Integritet** vælges **Vis backupdestinationer**, destination og
den eksplicitte bekræftelse af eksport af hele workspacet. **Dan og placér backup**
gemmer først et stabilt operation-ID i browseren. Serveren danner et valideret,
signeret snapshot og genkontrollerer den aktuelle autoritet før placering.

Arkivet får navnet `<operationId>.native-backup.json`, mode `0600` og publiceres
uden at overskrive en eksisterende fil. Serveren genlæser filen, kontrollerer
signatur, inventar, workspace-ID og SHA-256 og tilføjer en auditkvittering med
den autentificerede bruger og tidspunkt. Selskabernes posteringer ændres ikke.

Hvis svaret går tabt, bevares operation-ID'et også efter genindlæsning eller
ændret lokal serverport. Brug **Kontrollér placeringen**. Det er en ren læsning
af samme destination og ID; web-UI'et gentager ikke publiceringen automatisk.
Et eksisterende filnavn afvises med `backup_destination_exists_read_back`.
Ved `outcomeUnknown` eller `backup_destination_publication_unknown` skal samme
ID genlæses før en ny handling overvejes. En fil kan være landet, selv om
publiceringen eller auditkvitteringen ikke kunne afsluttes. `auditRecorded: false`
kræver operatørens afklaring; UI'et blokerer dannelse af næste backup. Slet eller
overskriv ikke en sådan fil for at få en grøn kvittering.

## CLI, MCP og HTTP

CLI-eksemplerne bruger en eksisterende normal MFA-session i en privat `0600`
cookiefil og den eksplicitte serveradresse. Brug kun `--allow-loopback-http`
ved lokal HTTP-drift. Opret og behold ét operation-ID før publiceringen:

```sh
rentemester-rust remote --server https://accounting.example --cookie-file /private/owner.cookie backup-destinations
rentemester-rust remote --server https://accounting.example --cookie-file /private/owner.cookie place-backup --destination-id company-backup --operation-id backup-20261021-001 --confirm
rentemester-rust remote --server https://accounting.example --cookie-file /private/owner.cookie verify-backup-placement --destination-id company-backup --operation-id backup-20261021-001
```

MCP-værktøjerne er `backup_destinations`, `place_backup` og
`verify_backup_placement`. De sidste to kræver `destinationId` og `operationId`;
kun `place_backup` kræver `confirm: true`. Ingen klient kan levere en sti,
arkivbytes, signeringsnøgle eller en selvvalgt actor som udvidet autoritet.

HTTP-operationerne er `GET /api/workspace/backup-destinations`,
`POST /api/workspace/backup-destinations/{id}/place` med
`{"operationId":"backup-20261021-001","confirm":true}` samt
`GET /api/workspace/backup-destinations/{id}/placements/{operationId}`.
De følger serverens eksisterende origin-, sessions- og MFA-kontrakt.

## Accept og resterende omfang

Group109 er verificeret på syntetiske data med normal ejeroprettelse og MFA:
faktisk webplacering, eksakt CLI/MCP-genlæsning, separat serververifikation af
signatur og SQLite-indhold, ingen overskrivning, afvist byteændring, anonym
afvisning, genindlæsning med samme ID, uændret bogføring og samme arkiv og audit
efter genstart. Den konkrete rapport er
`/tmp/rentemester-rust-reports/native-backup-destinations109c.json`, SHA-256
`6da2c4d095a00db756e8c4604ee5f8c9279832c311bb5da1efcf9458228166ad`.
En UI-test dækker mistet svar og efterfølgende eksplicit genlæsning; den faktiske
acceptprøve fremprovokerer ikke et procesnedbrud under publicering.

Dette forløb kvalificerer ikke ekstern cloudkopi, fuld gendannelse af det netop
placerede arkiv eller installeret desktop. Google Drive API-verifikation,
erklæret ekstern overførsel, brugeradministration af destinationer, friskhed,
backupstatus/-audit og eksplicit aktiveret bogføringslås er fortsat dele af den
samlede backup-paritet. `freshnessEvaluated` og `automaticBackupLock` er derfor
`false`. Desktopprøven er efter brugerens valg udskudt til allersidst.
