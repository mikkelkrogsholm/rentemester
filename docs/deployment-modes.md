# Driftsprofiler: lokal enkeltvirksomhed og hosted flerfirma

Status: implementeret produktgrundlag og resterende produktionsgates,
2026-08-24. Den lokale launcher og den isolerede hosted profil med Better Auth,
MFA, medlemskab og RBAC findes i kode. E-mailinvitationer, flere samtidige
workspace-ejere og credential-frit workspace-snapshot/restore er implementeret;
TLS/reverse-proxy og ekstern adversarial test er produktionsgates.

## Beslutning

Rentemester skal være ét produkt og ét dataformat med to tydelige
driftsprofiler:

1. **Rentemester Lokal** — én ejer, normalt én virksomhed, på ejerens egen
   computer uden ekstern serverdrift.
2. **Rentemester Hosted** — én kunde/organisation med én eller flere juridiske
   enheder i et server-hosted workspace, individuelle brugere og adgang pr.
   virksomhed.

En lokal virksomhed skal ikke bruge en simplere eller svagere regnskabskerne.
Forskellen ligger i installation, identitet, netværk og drift. Bogføring,
append-only-gates, audit, fakturaer, DigiSense, backupformat og
virksomheds-ledger skal være de samme.

Den første hosted arkitektur skal bruge **én deployment pr. kunde/organisation**.
Uafhængige kunder bør ikke dele workspace, identity database, volume eller
Rentemester-proces. En kunde kan derimod have flere selskaber i sit eget
workspace. Det giver en klar sikkerheds-, backup-, restore- og compliancegrænse
uden at bygge en kompleks, global SaaS-control-plane fra starten.

## Det nuværende fundament

Rentemester har allerede størstedelen af den nødvendige datagrænse:

- en virksomhed er en selvstændig mappe med egen `data/ledger.sqlite`,
  dokumentarkiv, fakturaer, konfiguration, logs, exports og backups;
- et workspace er en mappe med `workspace.json` og én virksomhed pr.
  undermappe/slug;
- `workspace.json` er den autoritative registrering: kun aktive `live`-poster
  med en unik, normaliseret CVR indgår i cockpit, portfolio og readiness.
  Test-, dry-run-, restore-, backup-, retest- og baseline-kopier må gerne
  ligge som søskendemapper, men bliver aldrig adopteret ved læsning;
- CLI'en kan fortsat arbejde direkte på en enkelt rå virksomhedssti med
  `--company <path>`;
- CLI og MCP kan slå en virksomhed op via slug, når et workspace er
  konfigureret;
- cockpittet og HTTP API'et er allerede workspace-scopet;
- portfolio-siden kan liste flere virksomheder og skifte via slug-baserede
  routes;
- Docker-imaget monterer ét workspace-volume og starter non-root;
- den samme core-funktion bruges af CLI, MCP og web.

Det betyder, at et workspace med én virksomhed og et workspace med ti
virksomheder allerede har samme grundform. Vi behøver ikke opfinde en ny
ledger-model for lokal brug.

## Profil A — Rentemester Lokal

### Målgruppe

En ejer eller selvstændig, der normalt arbejder med én virksomhed og ikke vil
drive en server, administrere domæne/TLS eller forstå containernetværk.

### Brugeroplevelse

Den ønskede oplevelse er:

1. installér Rentemester;
2. vælg “Opret virksomhed” eller “Åbn eksisterende virksomhed”;
3. Rentemester åbner cockpittet i browseren på `127.0.0.1`;
4. brugeren ser virksomhedens overblik direkte, ikke en unødvendig tom
   porteføljeoplevelse;
5. data ligger i en almindelig, dokumenteret lokal mappe, som brugeren kan
   tage backup af og eksportere fra.

Der kører stadig teknisk en lokal Bun HTTP-proces, fordi cockpittet er en
webapp, men det er ikke en ekstern serverdeployment. Processen må kun binde til
loopback og skal stoppe sammen med launcheren.

### Første lokale launcher

Den første konkrete launcher er CLI-kommandoen `local start`. Den kræver altid
en eksplicit datamappe og vælger aldrig en skjult standardplacering:

```sh
# Åbn et eksisterende workspace med præcis én aktiv virksomhed.
rentemester local start --workspace /sti/til/Rentemester-data

# Opret et nyt, almindeligt workspace og én virksomhed — først efter explicit
# actor og bekræftelse. `--no-open` er velegnet til headless brug.
rentemester local start --workspace /sti/til/Rentemester-data \
  --company-name "Eksempel ApS" --actor user:ejer --confirm yes --no-open
```

Kommandoen tvinger `127.0.0.1`, lokal deployment-profil og fravær af både
Better Auth og shared-token-auth, også hvis procesmiljøet indeholder hosted
variabler. Browseren åbnes først, når socketten er bundet, og en manglende
browser må ikke stoppe den lokale proces. Ved nul eller flere end én aktive
virksomhed afvises launcheren; flerfirma skal bruge den eksplicitte
`rentemester serve --workspace <dir>`-vej. En ikke-initialiseret, ikke-tom
mappe bliver aldrig adopteret automatisk.

### Lokal container som pakketeringsvariant

`local-container` er ikke en tredje produkt- eller dataprofil, men den
eksplicitte Docker-variant af Rentemester Lokal. Containerprocessen må binde
`0.0.0.0` internt, fordi Docker-bridgen ellers ikke kan nå den, men den
reviewede Compose-fil publicerer altid porten som
`127.0.0.1:4319:4319` på værten. Serveren afviser både en anden intern bind
under `local-container` og Better Auth-/hosted-secrets i denne profil.

Containerintegrationen initialiserer et workspace, genstarter via imagets
faktiske defaultkommando og kalder readiness gennem den publicerede
host-loopback-port. En operatør må aldrig ændre Compose-publiceringen til
`0.0.0.0`; netværks- eller interneteksponering kræver den fail-closed hosted
profil med Better Auth, TLS/reverse proxy og de øvrige produktionsgates.

### Datamodel

Lokal tilstand skal internt bruge et normalt workspace med én virksomhed:

```text
Rentemester-data/
  workspace.json
  min-virksomhed/
    data/ledger.sqlite
    documents/originals/
    invoices/
    config/
    backups/
```

Det gør en senere flytning til hosted enkel: hele workspacet eller den enkelte
virksomhedsmappe kan kopieres, verificeres og adopteres uden dataoversættelse.

CLI'en skal fortsat understøtte en direkte virksomhedssti. En lokal launcher
kan desuden sætte workspace og aktiv virksomhed for brugeren. Regnskabskommandoer
bør ikke indføre en skjult global standardvirksomhed; automatisering og agenter
skal fortsat vælge virksomhed eksplicit. UI'et kan derimod navigere direkte til
den eneste aktive virksomhed.

### Identitet og sikkerhed

Første lokale profil kan bruge operativsystemets bruger og loopback som
trust-grænse:

- bind kun `127.0.0.1`/`::1`;
- ingen eksponeret port på LAN eller internet;
- eksisterende Origin-, Content-Type-, actor-, confirm- og backup-lock-gates
  bevares;
- datafiler får private filrettigheder;
- lokale secrets kan senere lagres via operativsystemets credential store,
  men må ikke skrives i ledger eller rapporter.

Et lokalt login kan tilbydes senere, hvis flere personer deler samme computer,
men det må ikke være en forudsætning for den enkle én-ejer-oplevelse.

### Distribution

Der er to realistiske trin:

**Første leverance:** en lille lokal launcher/guide, som starter det samme
digest-pinnede GHCR-image via Docker Desktop eller en tilsvarende lokal
container-runtime. Det genbruger præcis de bytes, Digisense har reviewet, men
forudsætter, at brugeren kan installere en container-runtime.

**Bedre slutbrugerleverance:** et signeret native installationsartefakt pr.
platform, bygget fra samme SemVer og commit som containeren. Launcheren starter
Rentemester på loopback og åbner browseren. Et native artefakt har en anden
digest end OCI-imaget og skal derfor have egen provenance og indgå eksplicit i
compliance-scope; et review af containerbytes er ikke automatisk et review af
native bytes.

En Bun standalone-build kan undersøges som packaging-teknik, men må først
vælges efter bevis for `bun:sqlite`, migrationsfiler, regler/kilder, cockpit-
assets, dokumenter, backup/restore og platformssignering. Produktarkitekturen må
ikke afhænge af, at denne packaging virker.

## Lokal service-adgang

En lokal container bruger samme Better Auth API-key-format og samme
workspace-/virksomheds-memberships som hosted drift. Den har ikke en implicit
"lokal administrator-token" og en `actor` giver aldrig adgang alene.

Når et nyt lokalt workspace skal automatiseres, oprettes én servicekonto med
`workspace-access bootstrap-local-service`. Kommandoen kræver en præcis
virksomhedsrolle (`reader`, `reviewer`, `bookkeeper` eller `owner`), en
eksplicit `--confirm yes`, en policy-godkendt audit-actor samt en almindelig
0600-fil med den canonical base64url Better Auth-secret. Credentialet vises
kun i det succesfulde output og skal straks flyttes til en ekstern secret
manager. Det må aldrig skrives i workspace, ledger, image eller log.

`workspace-access local-service-rotate` erstatter én credential og viser den
nye nøgle én gang. `workspace-access local-service-revoke` deaktiverer den
valgte credential append-only. Begge kræver den samme lokale fysisk beskyttede
auth-secret-fil, live servicekonto-id, credential-id, virksomhed og audit-actor.
Ved hver MCP- eller CLI-write valideres credentialets enabled/expiry-status og
den konkrete servicekontos aktuelle workspace- og virksomheds-membership.

For `vendor-identity-enrichment apply` og `legacy-party-mapping apply` er den
mindste standardrolle `bookkeeper`, fordi den giver `company.master-data`;
`owner` virker også, mens `reader` og `reviewer` afvises. CLI/MCP-processen skal
have `RENTEMESTER_SERVICE_PRINCIPAL_TOKEN` og `RENTEMESTER_WORKSPACE` i sit eget
miljø. Inde i standardcontaineren er det canonical mount `/workspace`.
Credentialet er autorisationen. `--actor` er en separat, policy-godkendt
revisionsidentitet og giver aldrig adgang i sig selv. Plan/list er read-only;
apply kræver desuden eksakt plan-hash, idempotency key og `--confirm yes` (CLI)
eller `confirm: true` (MCP/HTTP).

Eksempel med et allerede initialiseret, syntetisk workspace-volume og en
eksternt opbevaret auth-secret-fil (samme secret som workspacets auth-runtime).
Erstat image-digest, volume, slug og actor med egne værdier; actor skal være
tilladt i virksomhedens policy. Secret-filen skal kunne læses af containerens
UID 1000, have mode 0600 og mountes read-only uden for `/workspace`:

```sh
docker run --rm --network none \
  --volume example-workspace:/workspace \
  --mount type=bind,src=/secure/auth-secret,dst=/run/secrets/auth-secret,readonly \
  ghcr.io/mikkelkrogsholm/rentemester@sha256:<verified-digest> \
  workspace-access bootstrap-local-service \
  --workspace /workspace --company example-company \
  --display-name "Bookkeeping service" --company-role bookkeeper \
  --auth-secret-file /run/secrets/auth-secret \
  --actor user:owner --confirm yes
```

Indlæs den returnerede service-token i klientprocessens miljø fra din eksterne
secret manager. Ved `docker run` videresendes miljøvariablen med
`--env RENTEMESTER_SERVICE_PRINCIPAL_TOKEN` (uden token i argumentet) og
`--env RENTEMESTER_WORKSPACE=/workspace`. Et eksisterende containershell arver
ikke automatisk værtens miljø. Gem også service-account-id og credential-id til
senere rotation/tilbagekaldelse. Kopiér ikke token-output til support/evidens.

Ledger-migration v52 tilføjer kun append-only enrichment-evidens og dens
indekser/triggere. Migrationen udfylder ingen leverandøridentiteter automatisk.
Afprøv upgrade på en kopi og verificér backup før drift; nedgradering til en
runtime uden v52 kræver en pre-upgrade-backup, ikke sletning af evidenstabellen.

## Hosted Better Auth: secrets og rate-limit-proxy

Dette er en opstarts-gate for hosted-profilen. Ingen af værdierne må logges,
lægges i workspace/ledger eller indgå i health-responsen.

### Rotation af secrets

`RENTEMESTER_AUTH_SECRET` er fortsat understøttet for en eksisterende
enkelt-nøgle-installation. Værdien skal være canonical, unpadded base64url for
mindst 32 kryptografisk tilfældige bytes. Runtime repræsenterer den som Better
Auth 1.7's version 1, så Better Auth ikke læser en ukontrolleret global
`BETTER_AUTH_SECRETS`-variabel.

For rotation sættes `RENTEMESTER_AUTH_SECRETS` som en komma-separeret, strengt
ordnet liste med `version:base64url`, fx `2:<ny>,1:<gammel>`. Første nøgle er
den aktive nøgle for ny Better Auth-kryptering og Rentemesters bootstrap-HMAC;
senere nøgler er kun til dekryptering/verifikation. Versioner skal være unikke,
ikke-negative heltal og alle nøgler skal opfylde samme encoding-krav.

Hvis payloads fra før den versionerede Better Auth-format findes, beholdes
`RENTEMESTER_AUTH_SECRET=<gammel>` under rotationsvinduet som legacy-fallback.
Efter en dokumenteret migrationsperiode kan den fjernes, men aldrig før de
relevante sessioner, callbacks og krypterede artefakter er udløbet eller
tilbagekaldt. Rotation er stadig en menneskelig driftsændring.

### Rate-limit og reverse proxy

Hosted kræver begge disse miljøvariabler:

- `RENTEMESTER_AUTH_RATE_LIMIT_IP_HEADER`, præcis én af `cf-connecting-ip` eller
  `x-real-ip`.
- `RENTEMESTER_AUTH_RATE_LIMIT_PROXY_CONTRACT=proxy-overwrites-client-ip-header-v1`.

Rentemester sender kun den valgte enkeltheader til Better Auth 1.7's
`advanced.ipAddress.ipAddressHeaders`; det bruger aldrig implicit
`X-Forwarded-For`, og det konfigurerer ikke `trustedProxies`. Better Auth kan
ikke fra en Bun-request bevise TCP-afsenderen. Derfor er den eksterne
deploy-gate ufravigelig: origin må kun være netværksmæssigt tilgængelig via den
valgte reverse proxy, og proxyen skal fjerne en klientleveret variant og selv
overskrive headeren med den observerede klient-IP. En direkte adgang til origin
eller en proxy der viderefører klientens header gør IP-baseret rate-limiting
spoofbar og må ikke sættes i produktion.

### Backup

Lokal tilstand bruger den samme generiske backup- og restoremotor. Produktet
kan gøre handlingen enkel (“Opret verificeret backup”), men destination,
synkronisering og retention forbliver driftens eller brugerens valg. Den lokale
bruger skal kunne få én signerbar backupfil og kontrollere restore uden at
kende databaseformatet.

## Profil B — Rentemester Hosted

### Deploymentgrænse

Første hosted model:

```text
Kunde/organisation A
  container + identity database + workspace-volume
    selskab-a/ledger.sqlite
    selskab-b/ledger.sqlite

Kunde/organisation B
  separat container + separat identity database + separat workspace-volume
    virksomhed-x/ledger.sqlite
```

Det giver:

- separat fejl- og kompromitteringsdomæne pr. kunde;
- entydig backup og restore;
- enkel sletning/eksport ved kontraktophør;
- mindre risiko for cross-tenant dataadgang;
- mulighed for at opgradere eller rollback-vurdere kunder separat;
- en tydelig containerdigest til compliance og drift.

En fremtidig control-plane kan administrere deployments, domæner, versioner og
status, men må ikke få direkte adgang til ledgers uden en særskilt og auditeret
kontrakt.

### Flerbruger og adgang

Hosted-profilen bruger Better Auth 1.7.1 mod en separat workspace-control-
database. Det tidligere fælles bearer-token er fortsat kun en eksplicit
legacy/test-seam og er ikke hosted-slutløsningen. Den implementerede model har:

- individuelle brugere;
- password/session/MFA/recovery;
- workspace-rolle;
- medlemskab pr. virksomhed;
- roller og permissions;
- workspace-audit for login, sessionsændringer, medlemskab og afviste forsøg.

Ansvarsgrænsen er bevidst: Better Auth ejer credentials, password-hash,
e-mailverifikation, sessioner, TOTP og recovery codes. Rentemester ejer den
append-only, effektivt reducerede adgang til juridiske enheder samt
regnskabsroller og permissions. Better Auths generelle organization-model er
derfor ikke sandhedskilde for selskabsadgang; den ville ikke i sig selv bevise
Rentemesters krav om én separat ledger pr. juridisk enhed og et uforanderligt
adgangsspor.

Hver HTTP-route autentificerer først brugeren og kontrollerer derefter
medlemskab/permission server-side, før sluggen omsættes til en virksomhedssti.
En bruger må aldrig kunne få adgang til et andet selskab ved at ændre URL eller
request. De eksisterende actor-, confirm-, append-only- og backup-lock-gates
kører fortsat efter authorization-gaten.

Minimumsrollerne er:

- administrator/ejer;
- bogholder;
- reviewer/godkender;
- læseadgang.

En ejer kan invitere en bruger til én konkret virksomhed og rolle. Invitationen
er e-mailbundet, kortlivet og single-use; den rå token gemmes aldrig. Flere
workspace- og virksomhedsejere er ligeværdige. Fjernelse eller deaktivering
afvises, hvis det ville efterlade workspacet eller en aktiv virksomhed uden en
effektiv ejer.

Hosted interneteksponering kræver desuden TLS/reverse proxy, sikre cookies,
CSRF, rate limiting, readiness, strukturerede logs og secret-management. Den
konkrete server, DNS, certifikater, backupdestination og retention hører til
privat drift, ikke Rentemesters generelle produktkode.

### Readiness og request-logs

`GET /api/health` er fortsat en let liveness-probe. `GET /api/ready` er den
offentlige, cache-frie readiness-probe: den læser det allerede oprettede
workspace-manifest, den checksummede workspace-control database og hver
registreret (også arkiveret) virksomheds ledger. Den migrerer aldrig, opdager
aldrig uregistrerede mapper og åbner alle SQLite-filer read-only med
`query_only`. Responsen er kun et samlet `ok`/`ready`, faste check-navne og et
antal; den indeholder aldrig stier, virksomhedsslugs, fejltekst eller secrets.
`200` betyder klar, `503` betyder ikke klar.

Serveren skriver ét allowlisted JSON request-completion-event pr. request med
tid, level, fast eventnavn, request-id, metode, rute-skabelon, status,
varighed og deploymentprofil. Den logger aldrig rå URL/query, IP, headers,
cookies, request-body, bruger-/virksomhedsidentitet, fejl/stack eller secrets.
`X-Request-Id` accepteres kun i et kort ASCII-format; ellers genereres et nyt
ID og returneres i responsheaderen. Log-sink-fejl ændrer aldrig svaret.

### Flere virksomheder

Det eksisterende workspace og cockpit understøtter allerede:

- oprettelse og arkivering af flere virksomheder;
- virksomhedsskift via slug-baserede URL'er;
- separate ledgers og dokumentmapper;
- et tværgående portfolio-overblik;
- workspace-kørsler for blandt andet recurring invoices og DigiSense inbound.

Hosted bruger nu Better Auth-sessioner og en central, server-side
membership/RBAC-matrix. Virksomhedslister, portfolio og alle deklarerede
virksomhedsruter filtreres, før en ledger åbnes; URL-/slug-skift giver derfor
ikke adgang til et andet selskab. Den resterende produktionsgate er den
konkrete reverse-proxy/TLS-verifikation, ekstern adversarial test og den
deployment-specifikke secret-, mail-, scheduler- og restore-drift — ikke den
grundlæggende virksomhedsisolation.

## Flere virksomheder er ikke det samme som en koncern

Rentemester kan i dag vise flere selskaber side om side og beregne enkelte
workspace-summer. Det er et portfolio-overblik, ikke et juridisk eller
regnskabsmæssigt konsolideret koncernregnskab.

Rentemester har nu en afgrænset koncernfunktion med:

- en effektivt dateret ejer-/virksomhedsgraf;
- ejerandele og relationstype som workspace-data;
- hver juridisk enhed i sin egen ledger;
- dokumenteret mellemregningsafstemning;
- eksplicitte elimineringer med evidens;
- read-only konsoliderede rapporter;
- tydelig skelnen mellem rå selskabstal, summer og konsoliderede tal.

Ingen konkrete CVR-numre, ejerandele eller kontomappings må hardcodes i
produktet. De konkrete relationer tilhører kundens workspace. Funktionen er
ikke lovpligtig koncernrapportering og understøtter endnu ikke
valutaomregning, minoritetsinteresser, indtægts-/omkostningselimineringer eller
skat.

### Koncernoverblik som selvstændig produktflade

Koncernoverblikket skal være adskilt fra portfolio-overblikket:

- **Portfolio** viser alle virksomheder, som brugeren kan tilgå. De behøver
  ikke have nogen juridisk relation.
- **Koncern** viser kun juridiske enheder, der indgår i en navngiven og
  effektivt dateret virksomhed-/ejergraf.
- **Konsolidering** er en beregnet read-only rapport med dokumenterede
  mappings og elimineringer. Den er ikke det samme som at summere selskaberne.

Den implementerede Cockpit-flade viser struktur, mellemregningsafstemning og
anvendte eliminationer. Følgende rigere kort, checklist og drill-down er næste
UI-slice; den underliggende profilbundne, konsoliderede rapport findes allerede
som read-only core-, CLI- og HTTP-kontrakt:

- koncernnavn, rapporteringsperiode og eventuel konsolideringsvaluta;
- ejerstruktur med moder-/datterselskaber, direkte og indirekte ejerandel samt
  relationernes gyldighedsdatoer;
- selskabskort med resultat, egenkapital, likviditet, moms/forpligtelser, åbne
  opgaver og drifts-/auditstatus;
- en mellemregningsmatrix med tilgodehavende, gæld, difference, alder og
  manglende modpost eller dokumentation;
- en konsoliderings-checkliste for regnskabsperioder, valuta, kontomapping,
  låste perioder, datakvalitet, mellemregninger og manglende elimineringer;
- status for elimineringer: foreslået, kladde, godkendt og anvendt i det
  read-only konsolideringslag;
- drill-down fra ethvert koncerntal til selskabsledger, kildeposteringer og
  elimineringsevidens.

Når grundlaget ikke er komplet, skal visningen fejle lukket: den kan vise
selskabstal og mangler, men må ikke vise et beløb mærket “konsolideret”. Når
konsoliderede tal senere vises, skal afstemningen være synlig som:

```text
Rå selskabssum
- dokumenterede elimineringer
± øvrige godkendte konsolideringsjusteringer
= konsolideret beløb
```

Koncernlaget må aldrig skrive på tværs af de juridiske enheders ledgers.
Elimineringer og konsolideringsjusteringer gemmes append-only i et særskilt
workspace-lag med actor, tidspunkt, status, godkendelse og referencer til
kildeposteringerne.

### Adgang og kontrakter for koncernoverblikket

Koncernmedlemskab giver ikke automatisk adgang til alle selskaber. Hver
forespørgsel skal håndhæve både workspace-/koncernrettighed og adgang til den
juridiske enhed. Hvis brugeren kun kan se en del af koncernen, skal visningen
tydeligt markeres som ufuldstændig og må ikke fremstille delsummer som hele
koncernen.

De generelle read-only kontrakter bør mindst dække:

- liste over koncerner og effektivt dateret struktur;
- koncernoverblik og konsolideringsparathed;
- mellemregningsafstemning med kildehenvisninger;
- rå summer, elimineringer og konsoliderede rapporter som separate felter.

Mutationer af ejerstruktur, mappings og elimineringer skal bruge de samme
actor-, confirm-, rolle-, review- og auditkrav som anden kontrolleret
bogføring. Konkrete selskaber, CVR-numre, ejerandele, kontomappings og
valutavalg er workspace-data og må ikke ligge i produktkoden.

### Leverancer for koncernfunktionen

1. **Struktur og overblik:** effektivt dateret koncerngraf, selskabsstatus og
   konsoliderings-checkliste; ingen konsoliderede tal.
2. **Mellemregninger:** generiske modparts-/kontomappings, reciprocal-balance
   rapport og dokumenteret mismatch-håndtering; ingen automatiske posteringer.
3. **Elimineringer:** kladde, review og godkendelse i et append-only
   konsolideringslag.
4. **Koncernrapporter:** read-only resultat og balance med fuldt drill-down og
   tydelig afstemning fra selskabstal til konsoliderede tal.

Koncernmoms, koncernskat og lovpligtig koncernrapportering er særskilte,
domæne-reviewede leverancer og følger ikke automatisk af dette overblik.

## Samme release, forskellige profiler

Funktionalitet og dataformat skal versioneres samlet:

| Egenskab | Lokal | Hosted |
| --- | --- | --- |
| Regnskabskerne | Samme | Samme |
| Virksomheds-ledger | Egen SQLite | Egen SQLite pr. juridisk enhed |
| Workspace | Normalt én virksomhed | En eller flere virksomheder |
| Netværk | Kun loopback | TLS via privat drift |
| Identitet | OS/localhost i første profil | Individuel auth + MFA + RBAC |
| Distribution | Lokal launcher/container | Digest-pinnet GHCR-image |
| Backupformat | Samme signerbare format | Samme signerbare format |
| DigiSense | Konfigureres pr. virksomhed | Konfigureres pr. virksomhed |
| Releaseidentitet | SemVer + artifact-digest | SemVer + OCI-digest |

Der bør ikke være et særskilt “single-company schema” eller en kodegren med
svagere invariants. Driftsprofilen vælges eksplicit ved opstart og påvirker kun
installation, navigation, auth og netværksgates.

## Migration mellem profiler

Flytning lokal → hosted skal være en kontrolleret import:

1. opret og verificér en signeret backup lokalt;
2. opret kundens isolerede hosted deployment på samme eller kompatibel nyere
   Rentemester-version;
3. restore til staging og verificér manifest, filer, schema og audit;
4. tilføj virksomhedsmembership og roller;
5. kontrollér DigiSense, email, scheduler og backups som separat hosted
   konfiguration;
6. skift først efter menneskelig godkendelse og en dokumenteret cut-over.

Hosted → lokal bruger samme backup/restorekontrakt. En ældre lokal runtime må
ikke åbne en ledger med et nyere schema.

## Hvad der mangler i produktet

### For en god lokal oplevelse

- direkte navigation til den eneste virksomhed;
- tydelig lokal dataplacering og backupknap/kommando;
- installations- og opgraderingsflow uden krav om serverkundskab;
- senere eventuelt signerede native builds.

Regnskabskernen og single-company CLI-flowet findes allerede.

### For hosted flerfirma

- sikker secret-injektion pr. deployment/virksomhed;
- driftskontrakt for scheduler, email, DigiSense, backup og restore;
- konkret reverse-proxy/TLS-verifikation og ekstern adversarial test;
- en dokumenteret upgrade/rollback-procedure omkring versionerede images.

### For egentlig koncern

- valutaomregning og besluttet konsolideringsvaluta;
- minoritetsinteresser;
- indtægts-/omkostningselimineringer og øvrige manuelle justeringer;
- koncernskat og lovpligtig koncernrapportering;
- særskilt review af disse regnskabsmæssige og juridiske antagelser.

## Resterende rækkefølge

1. Bevar de grønne dependency-, lint-, test- og automatiske sikkerhedsgates.
2. Fastlæg konkret secret-injektion og scheduler/email/DigiSense-drift.
3. Gennemfør reverse-proxy/TLS- og ekstern adversarial test.
4. Udgiv kun efter digest-bundet DigiSense- og menneskelig godkendelse.
