# Versionering og kompatibilitet

## Hvad der versioneres sammen

Rentemester-produktet består af runtime-kernen, CLI'en, MCP-serveren, HTTP
API'et og cockpittet i `app/`. De udgives som én enhed og deler samme SemVer.
Den kanoniske version står i rodens `package.json`; `app/package.json` skal
matche og kontrolleres af `bun run version:check`.

Marketing- og dokumentationssitet i `www/` er bevidst adskilt. Det har eget
package-manifest, egen lockfile, egen CI og egen deploy-livscyklus. En ændring i
`www/` kræver derfor ikke et nyt bogføringsimage, og `www` pakkes ikke i
Docker-imaget. Produktdokumentation, compliancekontrakter og changelog bliver
derimod i hovedproduktet, fordi de er evidens for den udgivne software.

## SemVer-politik

Git-tags har formen `vMAJOR.MINOR.PATCH`; package- og imageversionen har formen
`MAJOR.MINOR.PATCH`.

- `PATCH`: kompatibel fejl- eller sikkerhedsrettelse uden ændring af offentlige
  kontrakter eller datakrav.
- `MINOR`: ny bagudkompatibel funktion, nyt valgfrit felt eller ny migration,
  som ældre data kan opgraderes til. Før `1.0.0` bruges et MINOR-hop også til
  enhver kendt breaking change; den skal stå eksplicit i changelog.
- `MAJOR`: efter `1.0.0` enhver inkompatibel ændring af CLI, MCP, HTTP,
  manifests, reglernes fortolkning eller dataformat.

Regelbundlernes egne versionsfelter er faglig metadata. Den samlede
`rules.digest` er den autoritative content identity for en release og dækker
alle filer under `rules/dk/` og `sources/`.

## Databaseschema

Schema-version 1 er en checksummet baseline for hele det schema, Rentemester
havde, da produktversionering blev indført. Den immutable migrationsartefakt
er testmæssigt bundet til både de præcise `schema.sql`-bytes og den præcise
legacy-normalisering. Eksisterende databaser opgraderes tabsfrit og får først
baseline-rækken efter en vellykket normalisering.

Ved fremtidige schemaændringer skal der tilføjes en ny, append-only migration
med nyt id og stabil checksum. Software må aldrig ændre en allerede udgivet
migrations bytes eller genbruge dens id. En runtime afviser en database med et
højere migrations-id, en ændret checksum eller en manglende checksum i det nye
format, før den ændrer journal mode eller muterer databasen.

Det betyder også, at rollback ikke må gættes: efter en fremtidig ikke-reversibel
migration kan man kun gå tilbage med en verificeret backup eller en eksplicit,
testet down-migration. At starte et ældre image mod en nyere ledger er ikke en
rollbackstrategi.

## Versions- og image-tags

- Kandidat: `candidate-v0.2.0-<commit>-<workflow-run>-<attempt>`.
- Godkendt image: `ghcr.io/mikkelkrogsholm/rentemester:v0.2.0`.
- Uforanderlig driftsreference:
  `ghcr.io/mikkelkrogsholm/rentemester@sha256:<digest>`.
- Git/GitHub release-tag: `v0.2.0`.

Kandidatworkflowet giver hvert run sit eget tag og afviser genbrug af det i
workflowet. GHCR håndhæver ikke tag-immutabilitet over for alle fremtidige
administratorer, så tagget er en læsbar reference, ikke det endelige pin.
Kandidat-tagget og den tilhørende digest må bruges og deployes før
Digisense-review. Det er en teknisk, fastlåst distribution, men ikke en
compliance-godkendelse. Pin altid digest; kun den content-addressede digest gør
den valgte container-identitet direkte verificerbar.

Promoveringsworkflowet bygger ikke igen; det sætter versionstagget på den
allerede godkendte OCI-digest og verificerer bagefter, at digest er uændret.
`latest` indgår ikke i releaseflowet og må ikke bruges i drift.

## Sådan læses identiteten

```bash
rentemester --version
curl http://127.0.0.1:4319/api/health
```

MCP-klienter starter i compact-profilen med `system_server_about`; det
bagudkompatible full-profilnavn `meta_about` bevares. Health/MCP viser
produktversion, commit,
buildtid, schema-version/baseline-checksum og `rules.digest`. Lokale source-runs
har med vilje `null` for commit og buildtid; officielle images har begge dele.

Schema-kompatibilitet kontrolleres uden mutation af `system healthcheck`,
`system_healthcheck` og `/api/ready`. `/api/health` er alene liveness og
forbliver derfor 200, selv når readiness afviser en ventende eller korrupt
registreret ledger. Kun den actor-gatede CLI-kommando `system migrate --apply
yes` må opgradere schemaet; MCP har med vilje ingen migrations-tool.

Backups, myndighedspakker og SAF-T-pakker har `manifestVersion: 2` og samme
`provenance`-blok. Ældre backupmanifests uden feltet læses fortsat som v1.
