# Hjælp til ejer og serveroperatør

Denne guide gælder den fælles Rust-server og React-UI i web og Tauri-desktop.
Den erstatter ikke den resterende release- og cutoveraccept. Vælg først den
korrekte **Server:**-adresse og virksomhed. Ved en forbindelsesfejl skifter
appen ikke automatisk til et andet regnskab.

## Find det rigtige næste trin

| Det du ser | Som ejer | Som operatør |
| --- | --- | --- |
| Login eller MFA afvises | Brug den almindelige login-/recoveryvej; kontrollér korrekt server og bruger. | Kontrollér konfigureret auth-levering og aktuel adgang; opret ikke en ekstra ejer for at omgå fejlen. |
| Lokal server starter ikke | Bevar profilen og noter fejlkoden; luk appen normalt og kontakt operatøren ved vedvarende fejl. | Kontrollér profilbinding, rettigheder, en eksisterende ejerproces, optaget port og kompatibel identificeret binær. Bevar data og konfiguration. |
| Serveren er utilgængelig | Vent med nye writes og afklar eventuelle allerede afsendte handlinger. | Kontrollér proces, forbindelse og readiness for den valgte instans. Genstart med samme godkendte konfiguration, ikke et nyt workspace. |
| Data er flyttet eller skrivebeskyttet | Åbn den aftalte destination og kontrollér dens identitet. Historiske læsninger på den autoriserede kilde er ikke nye writes. | Genlæs kilde/målstatus. Fjern ikke tombstone eller genaktivér kilden; brug flytteforløbet nedenfor. |
| Kopi/schema/custody afvises | Bevar originalen; en mappekopi er ikke et nyt aktivt regnskab. | Følg den eksplicitte schema31→32-enrollment eller verificerede fresh restore. Slet ikke bindinger eller migrationshistorik. |
| Et bogførings- eller afsendelsessvar mangler | Find operationens oprindelige kvittering, journal/faktura og status. Send ikke igen med en ny nøgle. | Afklar kanonisk status under aktuel adgang; bevar ukendte providerudfald og accepterede effekter. |

Et grønt aggregate-healthsvar viser ikke, at en bestemt virksomheds regnskab er
korrekt. Brug også [regnskabskontrol](rust-audit-verify.md), relevant journal,
bilagsoriginaler og [revisionshistorik](rust-audit-log.md). En ukendt eller
manglende kontrol må ikke omfortolkes som bestået.

## Backup og tabt computer

Åbn [backupstatus og destinationer](rust-backup-destinations.md). Gem det
signerede arkiv og opbevar den betroede offentlige nøgle separat. Den offentlige
nøgle er ikke en hemmelighed, men en nøgle inde i et modtaget arkiv etablerer
ikke i sig selv tillid. Private signerings-, cookie-, MFA- og servicenøgler
hører ikke til i et supportmateriale.

Ejeren finder backup og separat tillidsmateriale. Operatøren verificerer det
præcise arkiv med den kontrollerede nøgle og klargør en frisk målmappe med nye
private runtime-secrets og normal ejeroprettelse/MFA. Følg
[serverens recoveryvej](rust-server.md#flyt-workspace-mellem-lokal-drift-og-server)
og backupvejledningens selvstændige gendannelsesforløb; flytning og recovery
ved utilgængelig kilde har forskellige eksplicitte flag og må ikke blandes.

Efter restore kontrollerer ejeren virksomhed, periode, bank, journal,
originalbilag og audit/inventory. Operatøren dokumenterer arkividentitet,
gendannelsespunkt og den målte tid for det faktiske forløb. En restore må ikke
kassere nyere accepterede posteringer. Gamle credentials og eksekveringssamtykker
bliver ikke automatisk aktive på målet. Afklar uafklarede eksterne forsøg;
afsluttede eller tvetydige mails/jobs må ikke blindt genleveres.

## Lokal↔server-flytning og flyttede data

Ejeren følger **Workspace → Flyt workspace**: destination/begrundelse,
stop/frysning, signeret arkiv, separat verificeret nøgle, målets udfordring,
kildens frigivelse og særskilt aktivering af netop målet. Operatøren håndterer
den friske målkonfiguration og verificerede staging. Den fulde
[flyttevejledning](rust-server.md#flyt-workspace-mellem-lokal-drift-og-server)
beskriver handlingerne og de oprindelige acceptbegrænsninger.

Ved tabt aktiveringssvar genlæses målets kanoniske tilstand. En aktiv tilstand
er ikke grund til at aktivere igen. En sikkert klargjort tilstand kan kun
genoptages gennem den oprindelige gennemgåede frigivelse. Kilden bliver ikke
skriver igen, fordi målet går ned. Nye accepterede målposteringer skal bevares
ved efterfølgende recovery og eksplicit afstemning.

[Fysisk custody i native schema32](architecture/rust-target/implementation/namespace-custody.md)
afviser rå kopier under aktuel software og beskriver stoppet, autoriseret
31→32-enrollment. Gamle31-data sammen med gamle31-binærer kan ikke tilbagekaldes
globalt af den lokale binding. Operatøren skal fortsat udpege én autoritativ
kilde, stoppe/dræne og håndtere gamle kopier/software under cutovergaten.
Den samlede installerede J06- og releaseaccept er stadig åben.

## Diagnose uden nye bogføringer

Brug den identificerede native binær. Serverens følgende opslag kræver hverken
workspace, credentials eller netværk og må stå alene:

```sh
/absolute/native/rentemester-server --build-info
```

Opslaget viser version, platform, API-epoch og control/company-schema; det
indeholder aktuelt ikke en kildecommit. Find kilde-/buildrevision og kontrolsummer
i den faktiske pakkes manifest, og sammenlign de to. Et mappenavn eller et
versionsnummer alene identificerer ikke kandidaten. En ældre, separat bevaret pakke kvalificerer sin egen revision;
den er ikke bevis for den aktuelle schema32-kandidat.

Offline virksomhedsdiagnose er en særskilt read-only kommando:

```sh
/absolute/native/rentemester-rust system healthcheck \
  --company /absolute/private/workspace/company --json
```

Den ændrer ikke originalen, migrerer ikke og giver ingen skriveautoritet. Læs
[CLI-kontrakten](cli-contract.md) for output og exitkoder: `0` er bestået,
`1` er et diagnostisk fund, `2` er en brugsfejl. Aggregate HTTP-health kan ikke
erstatte denne virksomhedsrapport. En fejl i schema, originaler eller adgang
løses gennem det relevante operator-/recoveryforløb, ikke en ekstra
bogføring, ændret SQL eller en svækket kontrol.

## Oplysninger til support

Der er endnu ingen kvalificeret automatisk supportbundle-collector eller
redaktionsgaranti. Lav en kort, manuelt gennemgået tekst/JSON med kun disse
oplysninger, fx:

```json
{
  "runtime": "Rust server + shared React",
  "build_revision": "PUBLIC_SOURCE_REVISION",
  "platform": "macos-aarch64",
  "mode": "local",
  "observed_at_utc": "2026-01-01T12:00:00Z",
  "stage": "ordinary-startup",
  "error_code": "SANITIZED_CODE",
  "readiness": "not-ready"
}
```

Tilføj om fejlen opstod før afsendelse, ved ukendt svar eller ved kanonisk
readback, samt de nødvendige ikke-hemmelige trin til at forstå den. Del ikke
hele konfigurationer, credential-/profilfiler, cookies, MFA/recovery codes,
providerkeys, rå logs, crashdumps, databasefiler, audit-/journaludtræk,
bankoplysninger, bilag, kunde-/brugernavne eller skærmbilleder med private data.
Er en fil eller fejltekst ikke sikkert afgrænset, udelades den; send ikke et
helt workspace for at spare en manuel gennemgang. Et særskilt autoriseret
regnskabs-/recoveryforløb er ikke en almindelig supportbundle.

## Runtime og supportstatus

| Del | Faktisk runtime | Statusgrænse |
| --- | --- | --- |
| Bogføring, adgang, server og CLI/MCP-klientkode | Rust og native afhængigheder | Eksakt release-/cutoverrevision skal stadig kvalificeres. |
| Web og Tauri-indhold | Samme byggede React-UI; browser/WebView på klienten | Endelig installeret/native/platformaccept udføres sidst. |
| Tauri-vært og administreret lokal server | Rust, platformens WebView og eksisterende lokale profil | Ingen anden bogføringsmotor eller automatisk reservebackend. |
| React-build og beholdte udviklings-/legacyværktøjer | Bun/Node/TypeScript efter værktøjets kontrakt | Ikke udtryk for afsluttet runtime-retirement. |
| Repositoryets MCP-wrapper | Fortsat Bun-wrapper | Vælg eksplicit native MCP-binær; #727s endelige entrypoint-/dependencyfjernelse afventer gates. |

macOS ARM64 og den registrerede Linux ARM64-kandidat har konkrete delbeviser.
Det er ikke en påstand om fuld support for andre arkitekturer/OS, signering,
opdateringer eller alle brugerforløb. [Releaseguiden](release/README.md) og
[epicens evidens](architecture/rust-target/implementation/progress.json)
fastholder de konkrete platform-, kompatibilitets- og versionskrav. Brug aldrig
en mismatchet pakke til at tvinge nyere data åbne.
