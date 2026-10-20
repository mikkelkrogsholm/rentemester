# Rust-server som container

`Dockerfile.rust` og `docker-compose.rust.example.yml` pakker Rust-serveren,
det eksisterende React-UI, Rust CLI/MCP og PDF-worker med det fastlåste PDFium.
Dette er den særskilte Rust-kandidat. Den eksisterende `Dockerfile` og
produktionsudgivelse bruger fortsat den tidligere runtime, indtil hele
cutover- og releaseaccepten er opfyldt. En lokalt bygget kandidat er ikke
en godkendt produktionsudgivelse.

## Byg kandidaten

Fra checkoutet:

```sh
python3 scripts/rust/cargo-guard.py status
docker build -f Dockerfile.rust \
  --build-arg RENTEMESTER_VERSION=0.2.0 \
  --build-arg RENTEMESTER_GIT_COMMIT="$(git rev-parse HEAD)" \
  --build-arg SOURCE_DATE_EPOCH="$(git show -s --format=%ct HEAD)" \
  --build-arg RENTEMESTER_SOURCE_DIRTY=true \
  -t rentemester-rust:candidate .
```

Brug kun `RENTEMESTER_SOURCE_DIRTY=false`, når et rent checkout er kontrolleret.
En ren label erstatter ikke et fuldt kilde-/image-bevis. Versionen ovenfor
er Rust-workspacets nuværende version, ikke en erklæring om offentlig release.
Bygning bruger digest-fastlåste Rust-, Bun- og Debian-images, dateret
GPG-verificeret Debian-snapshot og SHA256-fastlåst PDFium. Bun, Python og
Rust-toolchain er byggeafhængigheder; de kopieres ikke til runtime-imaget.
PDFium-licenser, Debian-pakkeversioner og kontrol af native biblioteker følger
med i `/opt/rentemester`.

BuildKit bruger én låst Cargo-cache pr. arkitektur. Den eksisterende
Cargo-kontrol kræver mindst 30 GiB ledigt og håndhæver én compilerproces uden
incremental-cache. `target/` er udeladt fra Docker-konteksten. Der bygges til
den valgte Linux-arkitektur; en bestået arm64-prøve beviser ikke amd64.
Uafhængig reproducerbarhed, SBOM/attestation og integration med den eksisterende
review-/promotionsrute er særskilte R53-gates.

## Konfiguration og første start

På Linux skal en dedikeret operatør klargøre en konfigurationsmappe, som UID/GID
`10001:10001` kan læse. Mappen skal være privat, og `server.json` skal være 0600.
Placér kun de nødvendige filer i denne mappe; den monteres read-only som
`/config`. Nøgler og konkrete virksomhedsdata tilhører installationen.

Eksempel på strukturen, som skal udfyldes med egne værdier:

```json
{
  "root": "/state/workspace",
  "brokerUid": 10001,
  "runtimeProfile": "dedicated-linux",
  "workspace": "example-workspace",
  "companySlug": "example",
  "companyName": "Eksempel",
  "cookieKey": "REPLACE_WITH_RANDOM_32_BYTE_BASE64URL_KEY",
  "mfaKey": "REPLACE_WITH_ANOTHER_RANDOM_32_BYTE_BASE64URL_KEY",
  "setupActor": "system:installer",
  "operatorActors": ["system:installer"],
  "accounts": [],
  "documentPdfWorker": {"bundleDir": "/opt/rentemester/bin"}
}
```

Generér to uafhængige tilfældige 32-byte nøgler og kod dem som base64url uden
padding. Gem dem og konfigurationen privat og bevar dem ved genstart.
Backup-signering og betroet restore-nøgle konfigureres separat efter den
eksisterende [server- og backupkontrakt](rust-server.md). Scanning og
fakturaudtræk kræver deres eksplicitte operatørkonfiguration; PDF-parsing
erstatter ikke malware-scanning eller regnskabsmæssig vurdering.

Sæt `RENTEMESTER_RUST_IMAGE` til det kontrollerede image ved præcis digest
og `RENTEMESTER_CONFIG_DIR` til den private mappe. Den navngivne datavolumen
monteres som `/state`, hvis 0700-ejerskab er klargjort i imaget. Det nye
workspace `/state/workspace` må ikke findes før første oprettelse.

```sh
docker compose -f docker-compose.rust.example.yml run --rm --service-ports rentemester --create
```

Den private installationsadresse i loggen bruges til normal oprettelse af
første ejer, e-mailbekræftelse og MFA. Del ikke installationsadressen eller
private bekræftelsesfiler. Stop den første proces kontrolleret og start
derefter almindelig drift:

```sh
docker compose -f docker-compose.rust.example.yml up -d
```

Almindelig start genåbner eksisterende data. Der udledes aldrig automatisk
`--create`, migration, restore eller aktivering af baggrundsjob. Forkert eller
manglende eksisterende workspace skal derfor afvises. Operatørkommandoer og
aktivering af workers følger fortsat deres eksplicitte serverkontrakter.

## Adgang, health og stop

Compose publicerer kun `127.0.0.1:4319`. Ekstern drift kræver TLS ved en
begrænset reverse proxy, korrekt kanonisk `RENTEMESTER_ORIGIN=https://…` og
konfigureret `authMail`-gateway. Bevar den kanoniske Host-header og videresend
kun via den tilsigtede proxy. Serveren ignorerer forwarded-IP-headere og
bruger den faktiske peer til ratekontrol; klienter bag samme proxy deler
derfor dens rate-bucket. Loopback-prøven kvalificerer ikke denne HTTPS-drift.

Processen kører som 10001 med alle capabilities fjernet, no-new-privileges,
umask 077 og read-only root. Kun datavolumen og den begrænsede tmpfs er
skrivbare. Health kræver både vellykket HTTP og `workspaceReady:true`;
`setupRequired:true` angiver stadig manglende ejeropsætning. Health er
opstartsstatus, ikke en ny komplet integritetskontrol af hver ledger.

SIGTERM afventer igangværende HTTP og optagne worker-runder. Compose tillader
fem minutter; justér fristen efter aktiverede workers og deres eksplicitte
providerbudgetter. Et tvungent stop er fortsat et crash, hvor et ukendt
eksternt udfald skal afstemmes før nyt forsøg.

## Syntetisk accept uden desktop

```sh
python3 scripts/rust/run-rust-container.py \
  --image rentemester-rust:candidate \
  --evidence /tmp/rust-container-evidence.json
```

Prøven bruger image-konfigurationsdigest, egne containere/volumener,
tilfældige nøgler og syntetiske regnskabsdata. Den prøver pakket PDF-parser,
normal web-ejer/MFA, bogføring, saldobalance, præcist originalbilag,
kontrolleret stop, genåbning og idempotent replay. Kontrollerer også manglende
Bun/Node/Python/Rust-toolchain i runtime og Linux-isolation. CLI/MCP køres
her kun med `--help`; deres tidligere forretningsevidens bevares særskilt.
Rapporten angiver faktisk arkitektur og grænser; ingen GUI- eller
produktionsaccept udledes af denne prøve.

## Pakket identitet og releasegodkendelse

Bygningen gemmer `/opt/rentemester/build-identity.json` med version, kildecommit,
dirty-markering, faktisk `rustc -vV` og Cargo-version, target, schema/API-policy,
base-image-digests og SHA256 for Cargo.lock, bun.lock, vendorkode, PDF-pins og
entrypoint. Inventaret binder hver pakket runtimefil med sti, størrelse,
SHA256 og executable-bit. Selve identitetsfilen er udeladt fra sit eget inventar.
Rust-, root- og webversion skal være ens; de er nu 0.2.0.

`rentemester-server --build-info` og `rentemester-mcp --catalogue` kræver ingen
konfiguration eller credentials. Førstnævnte bruger den kompilerede schema-
policy; sidstnævnte bruger den faktiske MCP-registry. Blandede argumenter
afvises. Kataloget dokumenterer interfaceidentitet, ikke fuld paritet med den
tidligere backend.

`scripts/release/create-rust-manifest.ts <build-identity.json>` producerer
manifest v2 ud fra den rå pakkede fil og de eksisterende `RELEASE_*`-variabler.
Dirty-kilde afvises. `RELEASE_BUILT_AT` skal være identitetens præcise UTC-tid.
Den fælles godkendelseskontrol kaldes for Rust med tre filer:

```sh
bun scripts/release/verify-approval.ts release-manifest.json approval.json build-identity.json
```

Kontrollen kræver identitetens præcise bytes samt menneskegodkendelsens præcise
manifesthash, image-digest, version og commit. Den eksisterende Bun v1-rute
bevarer sin kontrakt. Denne lokale v2-understøttelse ændrer endnu ikke CI eller
promotionsruten. Betroet workflow, SBOM, supply-chain, dækkende native
interfacekontrol, attestation og godkendt promotion af præcis digest skal stadig
forbindes og kvalificeres, før Rust bliver produktionsruten.

## Automatisk headless accept

`.github/workflows/rust-acceptance.yml` bygger Rust-pakken på Ubuntu amd64 ved
relevante PR-ændringer, push til main eller manuel start. Jobbet installerer
kun testbrowseren på værten, bygger `Dockerfile.rust` fra checkoutets præcise
commit/tid/version og kræver rent tracked kildeindhold. Det genbruger den
håndhævede Cargo-cache og pladsgrænse; det omgår ikke utilstrækkelig diskplads.

`scripts/release/verify-rust-package.py` kontrollerer det faktisk byggede image:
hele filinventaret og modes, entrypoint-hash, labels/arkitektur samt genkørt
server-/MCP-metadata. Tar-indhold læses uden udpakning, og links, dubletter,
traversal og uventede filrettigheder afvises. `--require-clean` afviser dirty-
kandidater, før der skrives bestået evidens. Identitets- og rapportfiler skal
være nye. Derefter gennemfører den eksisterende containerprøve normal
ejeropsætning/MFA, bogføring, rapporter, bilag, stop og genåbning. Scoped
rapporter og browserartefakter gemmes særskilt.

Dette CI-job publicerer ikke et image og ændrer ikke produktionspromotionen.
Den første fjernkørsel på Ubuntu/amd64 er endnu ikke verificeret; lokale arm64-
pakkekontroller beviser ikke dette. Det native discovery-katalog dækker foreløbig leverandørkøb. Konfigureret lokal/synkroniseret backupplacering og eksakt genlæsning er nu
implementeret og accepteret gennem web/CLI/MCP med genstart på macOS; se
[backupdestinationer](rust-backup-destinations.md). Containeren kræver en særskilt
beskyttet destinationsmount. Ekstern cloudverifikation, friskhed og fuld
backupstatus/-audit mangler stadig. Katalogets værktøjsantal må ikke bruges som grønt
paritetsbevis eller som erstatning for releasegatens dækningskrav.
