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

## Lokal opbevaring af dannede snapshots

Serverens almindelige signerede workspace-backup gemmes nu også som en privat
fil i workspacets faste `native-backups/`-mappe, før et downloadsvar afleveres.
Filnavnet er arkivets fulde SHA-256 efterfulgt af `.native-backup.json`. Mappen
har mode `0700`, filen `0600`; eksisterende bytes overskrives ikke. Serveren
genlæser og verificerer signatur, inventar, workspace og hash før registrering.
Dette kræver ingen ekstra operatørdestination og er lokal opbevaring.

Et efterfølgende append-only `native_backup_created`-event i workspace-auditten
registrerer arkivets identitet, størrelse, signeringsnøglens offentlige hash,
tidspunkt samt hver inkluderet virksomheds ledger-UUID og snapshot-hash.
Eventets actor kommer fra den aktuelle, allerede autoriserede menneskelige
session. En fil kan være landet uden en afsluttet auditregistrering; fejl efter
start af opbevaringen behandles derfor som uafklarede udfald. Filer fjernes ikke
automatisk ved mistet svar eller manglende kvittering.

De lokale snapshotfiler er ikke med i det signerede arkiv og bliver ikke til
en aktiv destinationskonfiguration ved gendannelse. Historiske auditkvitteringer
bevares, men inventarlæsningen tæller kun snapshots registreret efter den nye
instans' `native_portable_restore`-grænse. En kildekvittering beviser derfor ikke,
at en fil eksisterer hos den gendannede instans.

## Backupstatus og frivillig bogføringslås

Integritetsvisningen kan nu vise den aktuelle virksomheds lokale backupstatus.
Panelet **Backupstatus og bogføringslås** henter status eksplicit og lader en
aktuel virksomhedsadministrator gennemgå og bekræfte et låsevalg. Standard er
slået fra med nul ekstra fristdage. Den native kontrakt accepterer 0–365 ekstra
dage. Et uafklaret svar blokerer endnu en ændring, indtil brugeren manuelt har
hentet og kontrolleret status; der sendes ingen automatisk gentagelse.

Status tæller kun arkiver, som stadig kan genlæses fra den faste private mappe
og verificeres med den offentlige nøgle fra producentens separate, beskyttede
inventarkvittering. Nye kvitteringer gemmer den offentlige nøgles bytes og hash;
ingen privat nøgle eksporteres. Ældre kvitteringer uden separat nøgle kan ikke
kvalificere et arkiv alene. Manglende eller ændrede filer bliver ikke til en
brugbar backup. Status viser fejlene og bruger kun faktisk verificerede arkiver.
Låsekontrollen undersøger nyeste brugbare arkiv uden at genlæse alle ældre
arkiver ved hver bogføring; den eksplicitte status læser hele inventaret.

Aktivitet bygger på journalregistrering, bilagsindlæsning og bankdatoer. Rene
datoer vurderes ved dagens afslutning for seneste aktivitet og ved dagens start
for første aktivitet. De signerede snapshots bindes også til de inkluderede
rækkers identiteter, så en ny række ikke overses på grund af sekundpræcision.
Uden ny aktivitet siden backup opstår intet nyt forfald. Med ny aktivitet bliver
backup forfalden efter mere end syv dage. Uden nogen backup er aktivitet straks
forfalden, men låsens frist regnes fra første aktivitet plus syv dage og ekstra
frist. Låsen aktiveres kun, når brugeren har valgt den, backup er forfalden og
låsefristen er nået. Et nyt kvalificeret arkiv fjerner det aktuelle forfald.

Den fælles kontrol ligger efter aktuel autorisation og under begge
transaktionslåse før nye regnskabseffekter. Den dækker almindelig menneskelig
bogføring, servicekonti, leverandør-/salgs- og bankoperationer, åbningsbalance,
bilagspartsknytning samt den hostede fakturagenerering. Nye mailreservationer og
kontrollen før SMTP-aflevering bruger også låsen. Afslutning og afklaring af en
allerede påbegyndt aflevering kan fortsat registrere det faktiske udfald.
Læsninger, planer, backup, låseindstillinger og afklaring af eksisterende
kvitteringer er tilgængelige. Adgangskontrol gælder fortsat for dem.

Denne integration dækker endnu ikke samtlige tidligere mutationsgates:
kontrolbaserede virksomhedsoplysninger, selskabsdokumenter, ejerforhold,
koncern-/mellemregningsændringer og dele af mail-/workspace-indbakken mangler
fortsat tilsvarende integration. Der er ingen påstand om fuld låseparitet.
Disse restforløb skal afsluttes, før hele Rust-omskrivningen kan godkendes.

Låsevalg ligger som append-only virksomhedsaudit med serverafledt bruger og
en binding til den aktuelle gendannelsesgrænse. Historiske valg bliver ikke
aktive i en ny gendannet instans; en aktuel administrator skal vælge på ny.
Nøglerotation er ikke i sig selv en tilbagekaldelse af tidligere betroede
producentnøgler i inventaret. Et historisk arkiv kræver stadig de eksakte
verificerede bytes; den aktuelle signeringsnøgle bruges til nye arkiver.

Native HTTP bruger `GET /api/companies/{slug}/backup-status` og
`POST /api/companies/{slug}/backup-lock` med præcis
`{"confirm":true,"enforced":true,"graceDays":0}`. Status kræver aktuel
`CompanyAdmin`; integritetsoversigten kan vise status til en aktuel
virksomhedslæser. CLI er `remote backup-status --company SLUG` og
`remote backup-lock --company SLUG --confirm` med samme JSON på stdin, normal
serveradresse og privat cookiefil. MCP bruger `backup_status` med `company`
og `backup_lock` med `company`, `body` som JSON-streng samt `confirm: true`.
Ingen af disse klienter kan levere actor, nøgle, sti eller selvvalgt tidspunkt.
Den stærkere autorisation til backup af hele workspacet gælder fortsat.

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
erklæret ekstern overførsel, brugeradministration af destinationer og fuld
backup-governance/-audit er fortsat åbne dele af pariteten. Group111 forbinder
virksomhedens friskhed og finansielle lås; destinationernes separate liste
beregner stadig ingen samlet compliance og beholder `freshnessEvaluated: false`
og `automaticBackupLock: false`. Desktopprøven er efter brugerens valg udskudt
til allersidst.

Group110 kvalificerer den lokale opbevaring i det faktiske normale MFA-forløb:
to signerede snapshots med private filer, eksakte hashes og vedvarende
inventar efter genstart. Begge arkiver er også uafhængigt verificeret med
signatur- og SQLite-kontrol. Det oprindelige placerings-, afvisnings- og
bogføringsforløb består stadig. Rapport:
`/tmp/rentemester-rust-reports/native-backup-retention110b.json`, SHA-256
`69c448aa2c7a8b878fabb905f5b0f7a73a449408083674185a37dac399b11c59`.
Fem rene regeltests og to inventartests består; grænsen efter gendannelse er
dækket af inventartesten, ikke af et nyt faktisk source-free restore-forløb.
Den fulde tværgående låseparitet er fortsat uafsluttet; Group111 forbinder det finansielle forløb som beskrevet ovenfor.

Group111 kvalificerer nu det finansielle forløb med normal MFA: historisk
bankimport, faktisk webaktivering af låsen, afvisning uden regnskabseffekter via
menneskelig HTTP/CLI/MCP og servicekonto, læsning mens låst, særskilt bekræftede
låseændringer gennem både CLI og MCP mens låst, faktisk webbackup,
uafhængig signatur-/SQLite-verifikation, ophævet lås og præcis én postering med
samme kvittering efter genstart. Midlertidigt manglende og ændrede arkivbytes
afvistes, og præcis original fil blev genoprettet i testens `finally`-blokke.
Rapport: `/tmp/rentemester-rust-reports/native-backup-lock111b.json`, SHA-256
`63c53f87c8a1248753b6fb0100b24a0ad5681f1ecb49fd6174fe0320b69f49f9`.
Der var til slut én banktransaktion, én journalpost og nul bilag, og den ejede
syntetiske drift blev ryddet op. SMTP-afslutning under ny lås og
sekundpræcision er særskilt dækket af unit-tests. Den faktiske prøve kvalificerer
ikke ny nøgle efter rotation, SMTP, kontrolbaserede metadata, source-free
restore, ekstern backup eller installeret desktop.

## Ændringer i fælles virksomhedsdata (Group112)

Backupspærren kontrolleres også før nye ændringer i virksomhedsviden,
selskabsdokumenter, ejerforhold, parter, mellemregningsopsætning,
konsolideringsprofiler, elimineringer og mellemregningsdispositioner.
Fælles objekter kontrollerer alle berørte virksomheder. En erstatning tager
både tidligere og nye virksomhedstilknytninger med; en kilde, der kun læses,
bliver ikke en ekstra skrivevirksomhed.

De aktuelle rettigheder til hele ændringen kontrolleres først. Serveren holder
workspace-kontroltransaktionen, åbner de berørte virksomheders beskyttede
ledger-forbindelser efter tur og genopretter den oprindelige virksomhed før
svaret. En fejl under rollback eller genoprettelse gør ejeren ubrugelig frem
for at tillade usikker genbrug. Læsning, planer og eksisterende recovery beholder
deres kontrakter.

Mailindtag og workspace-indbakke mangler endnu deres tilsvarende integration.
En allerede accepteret ekstern handling skal stadig kunne registrere sit udfald;
en ny lås må ikke efterlade en faktisk udført handling uden kvittering.

Den faktiske Group112-prøve består med normal MFA: gyldige forslag til
virksomhedsviden og indlæsning af selskabsdokumenter afvises gennem HTTP, CLI og
MCP, mens låsen er aktiv. Alle seks afvisninger efterlader både metadata og
regnskab uændrede. Efter en verificeret backup lykkes det samme vidensforslag;
præcis ét forslag og nul afviste selskabsdokumenter genfindes gennem klienterne
efter genstart. De finansielle slutantal er fortsat nul bilag, én banktransaktion
og én journalpost. Rapport:
`/tmp/rentemester-rust-reports/native-backup-lock112.json`, SHA-256
`daf90aac72cbc81a8f98bc0aed6ef4842883c790a32982399a2273549988f01b`.
Alle 101 applikationstests består med otte særskilte hjælpetests ignoreret;
bygning og streng kodekontrol består. Prøven beviser ikke faktisk flerfirmadrift,
parter, mailindtag, SMTP, nøglerotation, ekstern backup eller installeret desktop.
