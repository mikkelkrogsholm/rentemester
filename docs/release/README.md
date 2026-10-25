# Release og Digisense-godkendelse

Releaseflowet sikrer, at Digisense vurderer de samme container-bytes, som senere
får et offentligt versionstag. Ingen af workflows må skabe `latest`.

## Engangsopsætning i GitHub

Repository-ejeren skal oprette environmentet `digisense-approval` og:

1. tilføje Digisenses udpegede GitHub-bruger/team som required reviewer;
2. slå prevent self-review til;
3. undlade administrator-bypass;
4. begrænse deployment branches til `main`.

Det er en ekstern, menneskelig gate og kan ikke håndhæves alene af filer i
repoet. Før environmentet er konfigureret sådan, må promotion-workflowet ikke
betragtes som en Digisense-godkendelse.

## 1. Forbered versionen

1. Vælg næste SemVer efter `docs/versioning.md`.
2. Opdatér `package.json` og `app/package.json` til samme version.
3. Flyt punkter fra `[Unreleased]` til en dateret sektion i `CHANGELOG.md`.
4. Kør `bun run version:check` og derefter `bun run verify:local`. Gem den
   afsluttende testopsummering sammen med releasearbejdet.
5. Merge ændringen til `main`. Opret ikke Git-tag manuelt.

## Rust-kandidaten og resterende accept

`release candidate` har nu et eksplicit `runtime`-valg, med `rust` som standard
og `bun` som den bevarede tidligere rute. Rust-versionen skal være ens i
`Cargo.toml`, `package.json` og `app/package.json`. Rust-kandidaten bruger
`Dockerfile.rust`, det samme React-UI og én fælles Rust-motor/server.

Rust-ruten kræver en Linux x64-runner med mindst 30 GiB ledigt på kilde- og
byggevolumen. Repositoryvariablen `RENTEMESTER_RUST_RELEASE_RUNNER` kan vælge en
allerede konfigureret runner med tilstrækkelig plads. Ellers anvendes
`ubuntu-latest`, og preflight stopper med en konkret kapacitetsfejl, hvis den
ikke opfylder kravet. Kontrollen må ikke sænkes for at få et grønt CI-job.
En faktisk egnet runner og fjernkørsel er endnu ikke kvalificeret.

To uafhængige kolde BuildKit-bygninger skal give samme OCI-arkivhash,
platformmanifest og image-konfiguration. Hver bygning bruger egen Cargo-cache.
Den første verificerede cache kan derefter genbruges til readiness og publicering.
Én fuld syntetisk ejer/MFA/bogføring/PDF/genstart-prøve køres mod det verificerede
image; det publicerede immutable image skal have de samme bytes. Det udløser
ikke en gentagelse af de samme finansielle effekter.

Samme kandidat-artefakt indeholder følgende originale filer med hver sin præcise
SHA-256-sidecar:

- `release-manifest.json` (v2)
- `rust-build-identity.json` (rå pakket identitet)
- `rust-package-evidence.json`
- `rust-license-evidence.json`
- `sbom.spdx.json` (den faktiske BuildKit-SPDX)
- `supply-chain-evidence.json`
- `agent-discovery-coverage-report.json`

`qualify-rust-candidate.py` kontrollerer det immutable image, platform,
pakkeinventar, rå identitet, licenspolitik og den faktisk vedhæftede SPDX.
Discovery kontrollerer de oprindelige MCP-inputsignaturer/sikkerhedshint og
workflowtrin; det erstatter ikke fuld semantisk CLI-/MCP-accept. Kendte mangler
og uafklarede forskelle mod de dokumenterede native kontraktkorrektioner må ikke
omdøbes til fuld dækning. Release forbliver afvist, indtil kravene er opfyldt.

Ved promotion bruges `--source source` til kandidatens godkendte kildecheckout.
Selve verifikatoren og den gældende licenspolitik kommer fra det betroede
promotionscheckout. Deterministiske supply-chain-/discoveryrapporter skal matche
originalerne; manifest, rå identitet, checksums, attestation, run-attempt og
uafhængig godkendelse skal alle passe. Der bygges ikke igen, og hverken `latest`,
overskrivning eller usikre registry-lookups er tilladt.

De lokale implementeringsprøver er bestået. Faktisk amd64-reproducerbarhed,
hosted CI/registry/attestation, HTTPS-proxy/cookies/klient-IP, recovery og
Digisense-godkendt promotion er fortsat åbne acceptkrav under #725. Dette er ikke
en erklæring om produktionsklar Rust eller en ændring af live-data.

De følgende ældre runtime-specifikke eksempler (pdf.js, lokal profil uden login
og `docker-compose.example.yml`) gælder `runtime=bun`. Brug for Rust
[Docker-/opsætningskontrakten](../rust-container.md), normal ejer/MFA og
`docker-compose.rust.example.yml`.

## 2. Byg release candidate

Start GitHub Actions-workflowet `release candidate` fra `main` med versionen
uden `v` og det fulde 40-tegns commit-id for den aktuelle `main`-HEAD. Workflowet
afviser et ældre commit, selv hvis det stadig kan nås fra `main`.

Workflowet:

- validerer version/commit og bruger commit-tidspunktet som reproducerbar
  buildtid;
- kører de hurtige kilde-, dependency- og buildkontroller, men gentager ikke
  de lokalt beståede backend- og cockpittests;
- bygger cockpittet; `www` har sit eget uafhængige workflow og kan hverken
  blokere eller ændre produktimaget;
- bygger to rene, timestamp-normaliserede OCI-exports og kræver identisk
  manifestdigest og arkiv-SHA-256;
- kører containeren non-root mod en ny persistent volume, kræver grøn
  readiness og verificerer en idempotent genstart; den samme smoke kører med
  read-only root, `--network none`, memory/CPU/PID/tmpfs-grænser og opretter
  syntetisk virksomhed/PDF gennem HTTP. Den verificerer rigtig pdf.js-tekst og
  layout, cache-genbrug samt det dokumenterede `no_text_layer`-udfald;
- bygger ét `linux/amd64` Docker-image og pusher kun kandidat-tagget;
- attesterer image-proveniens, før evidensartefaktet publiceres;
- publicerer en BuildKit-genereret SPDX-SBOM som OCI-attestation bundet til
  kandidatens immutable digest;
- udtrækker den samme SBOM til `sbom.spdx.json`, uploader den med egen SHA-256
  og binder checksummen ind i release-manifestet sammen med approval-schemaet;
  kandidaten afviser desuden en SBOM uden den låste `pdfjs-dist@6.2.108`.

Den fulde testsuite er en lokal releasegate, fordi den stærke udviklingsmaskine
kører den parallelt med Bun. Kandidatworkflowets ansvar er at bevise, at den
præcise commit kan bygges reproducerbart til et startbart Linux/AMD64-image og
at publicere netop dette image med attestering og evidens. Workflowet må derfor
ikke få `bun test`, `cockpit:test` eller den fulde kilde-smoke tilbage som en
skjult dobbeltkørsel.

Manifestet binder version, commit, OCI-digest, schema-checksum og regelsæt-digest
sammen med GitHub run-id og run-attempt. Digisense skal hente
kandidat-artefaktet fra workflow-runnet og teste imaget ved
`repository@sha256`, aldrig kun ved kandidat-tag.

### Brug kandidaten før review

En succesfuld kandidat er en færdig, anvendelig distribution. Den må installeres
og testes, selv om Digisense endnu ikke har reviewet den. Workflow-summaryen
viser både det entydige kandidat-tag og den autoritative digest:

```bash
docker pull ghcr.io/mikkelkrogsholm/rentemester@sha256:<candidate-digest>

export RENTEMESTER_IMAGE='ghcr.io/mikkelkrogsholm/rentemester@sha256:<candidate-digest>'
docker compose -f docker-compose.example.yml up -d
```

Denne deployment er fortsat **ikke reviewet af Digisense**. Der oprettes intet
Git-tag, GitHub release eller `vX.Y.Z`-image-tag på kandidatstadiet. Når samme
digest senere godkendes, promoveres de eksisterende bytes uden rebuild.

Kør en ikke-reviewet kandidat mod et nyt, isoleret workspace eller en verificeret
kopi af data. Peg den aldrig på den eneste live-ledger: åbning af en ledger kan
udføre de schema-migrationer, som kandidaten indeholder. Før afprøvning mod en
kopi af eksisterende data skal den aktuelle release derfor have produceret en
signeret backup, som også er restore-testet.

Compose-eksemplet bruger nye Docker-volumes og kan køre den eksplicitte lokale
profil uden login på hostens loopback (`127.0.0.1`). Den profil må ikke
eksponeres på LAN eller internet. En hosted deployment bruger Better Auth,
individuelle brugere, MFA og virksomhedsspecifik RBAC og kræver samtidig den
dokumenterede TLS/reverse-proxy-kontrakt. Indlæsning af eksisterende data i
kandidatens volume skal være en bevidst operation efter backupkontrollen ovenfor.

## 3. Indhent Digisense-godkendelse

Digisense udfylder JSON efter `digisense-approval.schema.json`. Følgende fire
felter skal være identiske med release-manifestet:

- `releaseManifestDigest`
- `imageDigest`
- `version`
- `gitCommit`

Godkendelsen er således ubrugelig for et andet build. JSON-filen er evidens;
GitHub-environmentets required reviewer er den autoritative identitetsgate.

## 4. Promovér uden rebuild

Start `promote approved release` med kandidatworkflowets run-id, run-attempt og
den komplette approval-JSON. Jobbet venter først på
`digisense-approval`-environmentet. Det
kræver derefter et succesfuldt kandidat-run fra den præcise trusted workflow på
`main`, henter kun det entydigt navngivne evidensartefakt og verificerer både
artefaktets checksum og image-attesteringen. Først når alle Git-, GitHub- og
GHCR-targets er preflightet, og attesteringens invocation-id er bundet til det
samme run-attempt, sætter det `vX.Y.Z` på samme GHCR-digest og opretter GitHub
release/tag på manifestets commit. Lookup-fejl behandles aldrig som "findes
ikke"; en autentificeret GHCR manifest-forespørgsel accepterer kun HTTP 404 som
fravær, og promotion stopper ved netværks-, auth- eller registry-usikkerhed.

Release-manifest, checksum og Digisense-approval vedhæftes GitHub-releasen.
Workflowet afviser et eksisterende Git-tag/GitHub release eller et versioneret
image med en anden digest. Release-workflows bruger commit-pinnede actions; en
opgradering af en action er derfor en eksplicit, reviewbar kodeændring.

Promotion bygger ikke igen. Den sætter kun et læsbart versionstag på samme
digest, så den digest-bundne SPDX-SBOM fra kandidaten bevarer præcis sit
oprindelige scope.

Git- og OCI-tags er navne, som en privilegeret administrator teknisk kan ændre
eller slette. Releaseprocessens preflight og publisher-politik forbyder det, men
driftsmæssig fastlåsning skal altid ske med `repository@sha256:<digest>`.

## Drift og rollback

Deploy altid den valgte digest fra release-manifestet. Versionstagget er læsbart
for mennesker, men digest er den fastlåste identitet. Hvis kandidaten ikke er
reviewet, skal den status følge deploymentet operationelt og må ikke omtales som
Digisense-godkendt.

Ved en applikationsfejl kan man vælge sidste godkendte digest, hvis dens runtime
understøtter ledgerens schema-version. Ved schemaændringer skal operatøren først
følge migrations-/backupplanen i `docs/versioning.md`; start aldrig blot et
ældre image mod nyere data. Tag en signeret, verificerbar backup før enhver
fremtidig schema-opgradering.
