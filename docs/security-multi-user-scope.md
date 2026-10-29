# Sikkert flerbruger-cockpit — status og implementeringsscope

Statusdato: 2026-08-23

## Formål

Dette dokument fastholder kodeundersøgelsen af, hvad Rentemester allerede har,
og hvad der mangler, før cockpittet kan bruges som et internetvendt
flerbrugerprodukt med sikker adgang til flere virksomheder.

Det er et scope- og statusdokument, ikke dokumentation for en gennemført
penetrationstest eller en erklæring om produktionsgodkendelse.

## Konklusion

Rentemester har nu et første, sammenhængende hosted sikkerhedslag baseret på
Better Auth 1.7.7: individuelle brugere, password-hash, verificeret e-mail,
TOTP med krypterede recovery codes, øjeblikkelig sessionstilbagekaldelse,
rate-limit, eksplicit Origin/CSRF-kontrol, medlemskab pr. virksomhed og en
central, testet permissionmatrix. Hosted webhandlinger tilskrives den konkrete
`user:<id>`-actor. Virksomhedslister, portfolio, filer, PDF'er og øvrige
virksomhedsruter filtreres og autoriseres, før den juridiske enheds ledger
åbnes.

Medlemsinvitationer, flere samtidige ejere og et portabelt, credential-frit
workspace-snapshot er implementeret. Der findes bevidst ikke et særskilt
"ejer-transfer"-workflow: en eksisterende ejer tilføjer en ny ligeværdig ejer,
og den tidligere ejer kan først fjernes, når hver aktiv virksomhed og
workspacet fortsat har mindst én effektiv ejer. Den generelle
kladde/review/godkendelsesmodel er
implementeret som append-only events i hver juridisk enheds ledger, med
atomisk godkendelse og bogføring af den eksakte indsendte version. Auth- og
adgangsevidensen er udbygget, men en samlet ekstern adversarial test og reel
reverse-proxy-verifikation er stadig produktionsgates.

Identitets- og virksomhedsisolationen er implementeret og testet i kode.
Interneteksponering er fortsat en produktionsgate, indtil den konkrete
reverse-proxy/TLS-kontrakt er verificeret, og en ekstern adversarial test er
bestået.

## Teknisk udgangspunkt

Rentemester kører på Bun:

- CLI, HTTP-server og MCP startes med `bun run`;
- SQLite-adgangen bruger `bun:sqlite`;
- kernetests køres lokalt med `bun test --parallel` via `bun run verify:local`;
- React-cockpittet bygges med Buns native bundler, og komponenttestene kører
  isoleret pr. fil med `bun:test`;
- produktimaget er baseret på `oven/bun`.

De centrale kilder for denne undersøgelse er:

- `src/server/auth.ts` — den fælles auth-seam;
- `src/server/mutations.ts` — virksomhedsmutationernes fælles gates;
- `src/server/actor.ts` — webprincipal til actor;
- `src/core/schema.sql` — append-only- og integritetsguards;
- `src/core/audit-log.ts` — auditlæsning og integritetskontrol;
- `src/core/documents.ts` — dokumentvalidering, hash og originalarkiv;
- `src/core/system-backups.ts` og `src/core/system-restore.ts`;
- `src/server/router/system.ts` — eksisterende health-endpoint;
- `docs/cockpit-api.md` — den nuværende HTTP-kontrakt;
- `Dockerfile`, `package.json` og `app/package.json`.

## Status mod ønskelisten

| Område | Status | Fund |
| --- | --- | --- |
| Individuelt login | Implementeret | Better Auth 1.7.7; offentlig signup er lukket. |
| Password-hashing | Implementeret | Credentials ejes og hashes af Better Auth, ikke af egen kryptokode. |
| TOTP MFA | Implementeret | Enrollment og login-challenge; første enrollment tilbagekalder password-only sessioner. |
| Recovery codes | Implementeret | Better Auths krypterede engangskoder og replay-test. |
| Password-reset | Implementeret | Better Auth-reset via den konfigurerede mail-seam; reset invaliderer eksisterende sessioner og telemetry-auditeres. |
| Sessioner | Implementeret | Personlige DB-sessioner, ingen cookie-cache og samlet straks-tilbagekaldelse. |
| Login-rate-limit | Implementeret med driftsgate | DB-baseret Better Auth rate-limit; hosted kræver en proxy-overwritten klient-IP-header. |
| Virksomhedsmedlemskab | Implementeret | Append-only workspace-control-events bestemmer aktuel adgang. |
| Roller/RBAC | Implementeret | Central, udtømmende permissionmatrix og route-catalog-test. |
| Virksomhedsskift | Implementeret | Vælger, virksomhedslister og portfolio viser kun aktuelle medlemskaber. |
| Bankposteringer | Findes | Cockpit og serverruter findes. |
| Bilag og dokumenter | Findes | Liste, upload, visning og dokumentkoblinger findes. |
| Bank/bilag-match | Findes | Matching- og bogføringsflows findes. |
| Bogførte posteringer | Findes | Journalvisning, rapporter og drill-down findes. |
| Fakturaer | Findes | Udstedelse, preview, bogføring og efterfølgende flows findes. |
| Saldi og rapporter | Findes | Resultat, balance, saldobalance og øvrige rapporter findes. |
| Mangler/undtagelser | Findes | Exceptions og agentforslag vises i cockpittet. |
| Generel bogføringskladde | Implementeret | Append-only versioner med præcis payload-/event-hash, dokument-/bankreferencer og Cockpit/CLI-flader. |
| Review/godkendelse | Implementeret for kladdeflowet | Bogholder indsender; en anden actor med `company.review` afviser eller godkender og bogfører atomisk. Direkte legacy-postering er fortsat en særskilt autoriseret vej. |
| Actor/confirm i web | Implementeret | Hosted handlinger bruger `user:<opaque-id>`; lokal profil bevarer cockpit-actor. |
| Append-only journal | Findes | Databaseguards afviser UPDATE og DELETE. |
| Korrektion/tilbageførsel | Findes | Rettelser sker gennem nye poster og reverseringer. |
| Invitationer og ejerkontrol | Implementeret | E-mailbundne engangsinvitationer, flere lige ejere og last-owner-guards for workspace og virksomheder. |
| Revisionsspor | Implementeret produktgrundlag | Domæne-, auth-state-, telemetry-, invitation-, medlemskabs-, dokumentadgangs- og kladde/review-evidens er append-only. |
| Dokumenthash | Findes | SHA-256 beregnes og bruges til deduplikering. |
| Filtypekontrol | Findes | Extension-allowlist og magic-byte-kontrol findes. |
| Uploadstørrelse | Findes | HTTP-upload er begrænset til 12 MiB inklusive base64-overhead. |
| Dokumentprivathed | Implementeret | Membership/permission før opslag; immutable fd-snapshot og sikker adgangsevidens. |
| Produktionsbuild | Findes | React-build og Docker-produktimage findes. |
| Healthcheck | Findes | `/api/health` returnerer service- og buildinformation. |
| Readiness | Implementeret | `/api/ready` verificerer kontrolschema og registrerede ledgers strengt read-only og returnerer kun aggregater. |
| Strukturerede logs | Implementeret for HTTP | En allowlistet completion-event pr. request med valideret request-ID og route-template. |
| Sikre migrationer | Findes overvejende | Checksums og transaktionelle schemamigrationer findes. |
| Backup/snapshot | Implementeret | Virksomhedsbackup samt credential-frit workspace-snapshot med signeret virksomhedsindhold og checksum-bundet samlet manifest. |
| Restore-verifikation | Implementeret | Staged restore kontrollerer signatur, filer, ledgerstatistik, auditintegritet og hele workspace-filinventaret før atomisk publicering. |
| Flerbrugersikkerhedstests | Første gate bestået | Cookie-E2E, MFA/recovery/replay, revocation, CSRF, secure cookies, route-permissions og cross-company afvisning testes. Ekstern adversarial test mangler. |

## Eksisterende dele, der skal genbruges

### Auth-seam og serverpipeline

Alle HTTP-kald passerer den samme auth-seam. Det er et godt udskiftningspunkt
for rigtig sessionsauth. Mutationer passerer desuden en fælles pipeline med:

1. localhost-, Origin- og Content-Type-kontrol;
2. virksomhedsresolution;
3. `confirm: true` for irreversible handlinger;
4. databaseåbning og migration;
5. backup-lock;
6. actor-resolution;
7. kald til den fælles regnskabskerne;
8. sikker fejlkonvertering.

Der skal bygges videre på denne pipeline frem for at lægge authorization
spredt ud i de enkelte handlers.

### Append-only bogføring

`journal_entries` og `journal_lines` er beskyttet mod UPDATE og DELETE med
database-triggere. Reverseringer er nye, eksplicit sammenkædede journalposter.
Udstedte fakturadokumenter og en række afledte bogføringsobjekter har tilsvarende
append-only-guards.

Denne model skal bevares. Flerbrugerarbejdet må ikke introducere en alternativ
webvej, der kan skrive direkte uden om kernen.

### Audit

Hver virksomheds database har et append-only `audit_log` med handling,
entity-reference, actor og tidspunkt. Integritetskontrollen opdager blandt
andet huller og journalposter uden auditbevis.

Det nuværende auditspor er egnet som domæneaudit, men ikke som komplet
flerbrugeraudit, fordi login, session, medlemskab og adgangsafvisninger ligger
uden for den enkelte virksomheds ledger.

### Dokumenter

Dokumentkernen har:

- SHA-256 af originalen;
- unik hash i databasen;
- tilladte typer: PDF, PNG, JPEG, tekst, JSON og XML;
- magic-byte-kontrol for binære formater;
- logisk dubletkontrol;
- separat originalarkiv;
- audit ved ingest;
- integritetskontrol af filens hash.

HTTP-upload har en fast requestgrænse. Den eksisterende dokumentpipeline skal
bevares og beskyttes med membership/permission-checks.

### Backup og restore

Rentemester har allerede en generisk backupmotor, der ikke kræver en bestemt
backupdestination. Den producerer manifest, checksums og signaturbevis. Restore
foregår via staging og verificerer både artefakter og den gendannede ledger.

Driften skal fortsat eje destination, tidsplan og retention.

## Målarkitektur

### Workspace-identitet — implementeret fundament

Rentemester bruger en separat workspace-control-database til identitet,
adgangsevents og workspace-audit. Better Auths tabeller ligger ved siden af
Rentemesters checksummede, append-only adgangstabeller; de blandes aldrig ind i
de enkelte virksomheders journaler.

Den samlede identity-grænse modellerer nu:

- brugere;
- credentials/password-hash;
- sessions;
- MFA-konfiguration;
- hash'ede recovery codes;
- password-reset og e-mailbundne engangsinvitationer;
- virksomhedstilknytninger;
- roller;
- workspace-audit.

Hver juridisk enhed beholder sin egen ledger. Konkrete virksomhedsdata,
CVR-numre og lokale policyvalg forbliver workspace-data og må ikke hardcodes i
produktet.

### Central authorization

Authorization skal have formen:

```text
principal + virksomhed + permission -> allow eller deny
```

Alle HTTP-ruter skal deklarere en permission. En kontrakttest skal fejle, hvis
en ny rute tilføjes uden permissiondeklaration.

Alle opslag skal kontrolleres server-side. Det gælder også:

- virksomhedslister og portfolio;
- dokumentvisning og download;
- faktura-PDF'er;
- rapport- og myndighedseksporter;
- mutationer;
- objekt-ID'er, der indsendes i request body eller URL.

Et afvist kald bør ikke afsløre, om et objekt findes i en virksomhed, brugeren
ikke har adgang til.

### Minimumsroller

| Permission | Ejer/admin | Bogholder | Reviewer | Læseadgang |
| --- | ---: | ---: | ---: | ---: |
| Se regnskab og bilag | Ja | Ja | Ja | Ja |
| Uploade og matche | Ja | Ja | Nej | Nej |
| Oprette og indsende kladde | Ja | Ja | Nej | Nej |
| Godkende eller afvise | Ja | Nej | Ja | Nej |
| Bogføre godkendt kladde | Ja | Nej | Ja, kun gennem review-flowet | Nej |
| Korrigere/tilbageføre | Ja | Med godkendelse | Nej | Nej |
| Administrere medlemmer | Ja | Nej | Nej | Nej |
| Administrere virksomhed | Ja | Nej | Nej | Nej |

Den endelige permissionmatrix skal være central og testbar; handlers må ikke
afgøre roller ad hoc.

## Implementeringsfaser

### Fase 1 — identitet og sessioner (implementeret)

- Integrér et gennemprøvet auth-bibliotek; skriv ikke egen passwordkryptografi.
- Implementér individuelt login og sikker password-hash.
- Implementér TOTP enrollment og challenge.
- Krypter TOTP-secrets med en nøgle leveret af driften.
- Gem recovery codes som hashes og forbrug dem præcis én gang.
- Implementér sikre cookie-sessioner med rotation, idle timeout og absolut udløb.
- Implementér logout af én session og tilbagekaldelse af alle sessioner.
- Tilbagekald sessioner ved passwordskift og brugerdeaktivering.
- Rate-limit login pr. konto og klientkilde uden brugerenumeration.
- Bootstrap, e-mailbundne engangsinvitationer, deaktivering og password-reset
  er implementeret. Inviteret adgang forbliver lukket, indtil e-mail og TOTP
  er klar. Sessioner har 12 timers absolut levetid uden skjult refresh; følsomme
  mutationer kræver en højst 10 minutter gammel session.

Cookie-sessioner ændrer trusselsmodellen: den nuværende Origin-kontrol må ikke
slås fra, blot fordi auth er aktiveret. Cookie-authenticated mutationer skal
have konsekvent Origin/CSRF-beskyttelse.

### Fase 2 — medlemskab og RBAC (implementeret)

- Implementér virksomhedsmedlemskaber og roller i workspace-databasen.
- Udvid `Principal` med rigtig brugeridentitet og sessionsidentitet.
- Lad webactor være `user:<id>` frem for den fælles cockpit-actor.
- Tilføj central authorization før virksomhedsdatabasen åbnes.
- Filtrér virksomhedslister og portfolio efter medlemskab.
- Beskyt alle virksomhedsruter, downloads og eksporter.
- Tilføj route-inventory-testen, der kræver permissionmetadata.

### Fase 3 — weboplevelse (implementeret første komplette snit)

- Login- og MFA-skærme.
- MFA-opsætning og recovery-flow.
- Sessionoversigt og “afslut alle sessioner”.
- Bruger- og medlemsadministration for ejer/admin.
- Virksomhedsvælger, der kun viser tilladte virksomheder.
- Rollevisning, udløbet-session-flow og entydig adgang-afvist-side.

De eksisterende bank-, dokument-, journal-, faktura-, rapport- og
undtagelsesviews genbruges.

### Fase 4 — generelle bogføringskladder (implementeret første komplette snit)

Indfør en persistent, generisk workflowmodel:

```text
kladde -> indsendt -> godkendt og bogført
                  \-> afvist -> ny kladdeversion
```

Godkendelse og bogføring bør være atomisk, så systemet ikke efterlader en
uklar mellemtilstand, hvor en kladde er godkendt, men endnu ikke bogført.

Modellen skal mindst indeholde:

- kladdehoved;
- append-only kladdeversioner og linjer;
- dokument- og bankreferencer;
- opretter og indsendelsestidspunkt;
- reviewer, beslutning og begrundelse;
- reference til den endelige journalpost;
- databasebeskyttede statusovergange.

Eksisterende dry-run-validering bruges til preview, og den eksisterende
bogføringskerne forbliver eneste vej til journalen.

Den implementerede model ligger i `src/core/accounting-drafts.ts` og schema
v9. `created`, `revised`, `submitted`, `rejected` og `approved_posted` er
immutable events i én valideret hash-kæde. Submit og review kræver den eksakte
event-hash; reviewer skal være en anden canonical actor end både versionsforfatter
og indsender. `approved_posted` og journalposten skrives i samme
`BEGIN IMMEDIATE`-transaktion, og et retry af samme indsendte identitet
returnerer eksisterende journalbevis uden at bogføre igen. Hosted API'et
håndhæver `company.draft.write` og `company.review` før ledgeren åbnes, mens
CLI'en håndhæver actor-allowlist, backup-lock og `--confirm yes` ved posting.

### Fase 5 — revisionsspor (implementeret produktgrundlag)

Udvid domæneaudit med strukturerede felter eller et versioneret details-payload
for:

- user, program og request-ID;
- virksomhedskontekst;
- handling og resultat;
- entity-type og entity-ID;
- før/efter for mutable masterdata;
- reference til append-only resultater.

Tilføj workspace-audit for:

- login succes og fejl;
- MFA og recovery-code brug;
- logout og sessionstilbagekaldelse;
- rate-limit;
- invitation, deaktivering og password-reset;
- medlemskabs- og rolleændringer;
- afvist virksomhedsadgang.

Passwords, sessioncookies, tokens, TOTP-secrets og recovery codes må aldrig
optræde i logs eller audit.

### Fase 6 — dokumenthardening (implementeret)

- Kræv permission på visning og download.
- Tilføj et generisk malware-scanning-hook.
- Fejl lukket, hvis scanning er obligatorisk, men ikke kan udføres.
- Opret originalfiler eksklusivt, så eksisterende bytes aldrig overskrives.
- Test traversal, symlinks, filnavne og MIME-manipulation.
- Auditér upload, download og afvisning uden at logge dokumentindhold.

### Fase 7 — driftsklarhed (implementeret produktgrundlag)

- Bevar produktionsbuild og Docker-image.
- Del health i liveness og readiness.
- Readiness skal kontrollere auth-store, migrationsstatus og nødvendige
  workspace-forudsætninger uden at lække paths eller secrets.
- Indfør strukturerede JSON-logs med request-ID og central redaktion.
- Lad startup stoppe sikkert ved auth- eller ledgermigrationsfejl.
- Det credential-frie workspace-snapshot bevarer kun navn, e-mail og
  rolleplan som privat recovery-evidens. Password-hashes, sessioner,
  TOTP-secrets, recovery codes, tokens og provider-credentials kopieres ikke;
  efter restore bootstrapper en ejer og geninviterer brugerne.

## Sikkerheds- og acceptgate

Flerbrugerdrift er først klar, når automatiske tests beviser mindst følgende:

- anonym adgang afvises;
- deaktiveret bruger afvises;
- bruger A kan ikke se eller ændre virksomhed B;
- manipulation af slug, objekt-ID, dokumentdownload, PDF eller eksport afvises;
- alle roller afprøves mod alle permissions;
- alle virksomhedsruter har permissionmetadata;
- MFA enrollment, login og replay-afvisning virker;
- recovery codes kan kun bruges én gang;
- sessionrotation og “afslut alle sessioner” virker straks;
- passwordskift tilbagekalder sessioner;
- rate limiting virker uden brugerenumeration;
- cookies har korrekte sikkerhedsattributter;
- cookie-authenticated mutationer er CSRF-beskyttede;
- login, upload, match, godkendelse, bogføring, korrektion og eksport auditeres;
- auditsporet er append-only;
- bogførte poster kan ikke opdateres eller slettes;
- originale dokumenter kan ikke overskrives;
- backup kan verificeres og gendannes med identitets- og ledgerintegritet.

Derudover skal der gennemføres en særskilt adversarial gennemgang af
authorizationgrænsen og sessionsdesignet før interneteksponering.

## Foreslået issue-opdeling

1. Workspace identity schema og auth-integration.
2. Password, MFA, recovery og session lifecycle.
3. Virksomhedsmedlemskaber og central permissionmodel.
4. Server-side authorization på samtlige HTTP-ruter.
5. Login-, session-, medlems- og virksomhedsvælger-UI.
6. Generisk kladde-, review- og approvalmodel.
7. Struktureret domæne- og workspace-audit.
8. Dokumentauthorization og originalfil-hardening.
9. Readiness og strukturerede logs.
10. Samlet adversarial sikkerhedstest og produktionsgate.

Afhængighed:

```text
identitet
  -> medlemskab og RBAC
  -> login-UI og virksomhedsskift
  -> kladde/godkendelse og audit
  -> dokument- og driftshardening
  -> samlet sikkerhedsgate
```

## Uden for Rentemester-produktet

Følgende tilhører den private driftsopsætning og må ikke hardcodes i
Rentemester:

- Hetzner-konfiguration;
- konkrete servernavne og domæner;
- Google Drive som obligatorisk backupdestination;
- backupplan og retentionplan;
- konkrete CVR-numre, ejere, kontoplaner, bankkonti eller saldi;
- private repositories;
- produktionscredentials og secrets;
- den konkrete deploymentprocedure.

Rentemester skal levere generelle, sikre mekanismer, som den private drift kan
konfigurere og anvende.
