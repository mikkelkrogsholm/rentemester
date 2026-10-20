# Native Rust-pakker og opdateringer

Opdatering er en separat, offline pakkeoperation. Den starter ingen server,
åbner ingen virksomhedsdata og udfører ingen migration eller automatisk rollback.
Den nuværende pakke og kandidaten skal begge være autentificeret med en
uafhængigt valgt Ed25519-opdateringsnøgle. En usigneret udviklingsinstallation
bliver ikke stiltiende en betroet baseline.

Dette er en del af R52. Apple Developer ID/notarisering, de øvrige platforme,
installeret GUI-opdatering og den fulde cutover/ældre-software-gate er fortsat
åbne. En Ed25519-opdateringssignatur er ikke Apple-kodesignering.

## Fremstilling

Den eksisterende `scripts/release/assemble-native-macos.py` pakker nu tre
byggede Rust-binærer: `rentemester-desktop`, `rentemester-server` og
`rentemester-update`, plus det eksisterende web-UI og det godkendte appikon.
Den bygger eller starter intet. Alle tre binærer skal være til stede i den
valgte fælles Cargo-cache. Manifestet angiver de faktiske toolchains og SHA256
for pakkens filer; indsend ikke en påstand om en toolchain, som ikke er anvendt.

Med alle tre flag `--update-signing-key PRIVATE_PKCS8`,
`--update-public-key INDEPENDENT_RAW32_PUBLIC_KEY` og
`--release-sequence POSITIVE_INTEGER` oprettes desuden et autentificeret
opdateringsmanifest. Output skal da være en ny pakke, aldrig en eksisterende
installation. Alternativt signer en allerede samlet, frisk pakke:

```sh
python3 scripts/release/sign-native-update.py \
  --bundle /absolute/Candidate.app \
  --key /absolute/private-update-key.pem \
  --public-key /absolute/trusted-update-public.raw \
  --sequence 2 --platform macos-aarch64 \
  --api-epoch 1 --control-schema 31 --company-schema 36
```

Den private nøgle skal være en beskyttet, regulær fil og må ikke pakkes med
produktet. Signeringsværktøjet kontrollerer, at den svarer til den særskilt
angivne offentlige nøgle. Det nægter at overskrive eksisterende metadata.
Produktversion og kilderevision kommer fra pakkens artefaktmanifest.

## Installation og opdatering

Første installation og tillid til opdateringsnøglen skal etableres gennem den
særskilt godkendte distribution. Værktøjet henter intet fra netværket og vælger
ikke selv en nøgle eller kandidat. Brug kanoniske, absolutte stier. Nøglen er
32 rå offentlige bytes; den skal være en regulær, beskyttet fil uden symlinks.

```sh
rentemester-update verify --package /absolute/Candidate.app \
  --trusted-key /absolute/trusted-update-public.raw
rentemester-update status --installed /absolute/Rentemester.app \
  --trusted-key /absolute/trusted-update-public.raw
rentemester-update apply --package /absolute/Candidate.app \
  --installed /absolute/Rentemester.app \
  --trusted-key /absolute/trusted-update-public.raw --confirm
```

Luk appen og dens lokale server først. Både den nye desktop og en server startet
fra pakken holder en delt installationslås; opdatering kræver den eksklusive
lås. Udviklingsbinærer uden for en app-pakke berøres ikke. Der bruges ingen
skjulte credentials eller baggrundsopdatering.

Signaturen dækker præcise manifestbytes og alle desktop-/server-/updater-/webfiler.
Manglende, ekstra, ændrede eller sammenkædede filer, symlinks, ændret eksekverbarhed
og usikre skriverettigheder afvises. Kandidatens platform, API-epoch og begge
schemaer skal matche opdateringsværktøjets kontrakt. Både produktversion og
release-sekvens skal være højere end den installerede. Schemaovergange kræver
særskilt verificeret operatørarbejde; opdateringen gætter ingen datamigration.
Eksisterende database-adgangskontroller for nyere/ændrede schemaer bevares.

En komplet kandidat kopieres og genverificeres i en privat mappe på samme
filsystem. Et atomisk directory-exchange skifter pakken uden et mellemrum,
hvor installationsstien mangler. Begge berørte mapper synkroniseres til disk.
Den tidligere pakke bevares ved siden af installationen; en endnu ældre,
autentificeret pakke fjernes først ved næste opdatering. Dermed bevares højst
én færdig tidligere generation. En fremmed eller ændret tidligere pakke
slettes ikke automatisk. Stage og tidligere generation er ikke aktive
startsteder for de nye desktop-/server-binærer; ingen automatisk rollback.

## Afbrudt operation

Læs altid `status` efter et tabt eller ukendt svar. Gentag ikke blindt `apply`.
Hvis `pendingRecovery` er sand, vælg eksplicit:

```sh
rentemester-update recover --installed /absolute/Rentemester.app \
  --trusted-key /absolute/trusted-update-public.raw --confirm
```

Før publication kasseres kun den private staging-transaktion; den gamle
installation forbliver uændret. Efter publication færdiggøres opbevaring af
den gamle pakke og oprydning. Ukendt eller ændret evidens afvises med bevarede
filer. `update_outcome_unknown` betyder, at installationsstien kan være skiftet;
brug den præcise manifesthash og release-sekvens fra `status` til afklaring.

Exit 0 betyder kendt succes med `ok:true`; exit 2 betyder forkert kommandokald;
exit 1 betyder afvist eller uafklaret pakkeoperation. Ingen af koderne giver
implicit tilladelse til at ændre virksomhedsdata.

## Verifikation

`run-native-update.py` er et syntetisk headless acceptforløb med eksplicitte
binærstier og ny rapportsti. Det bruger midlertidige testnøgler og pakkeversioner,
normal Mac-bruger, faktisk proceslås, faktisk SIGKILL under staging og eksplicit
recovery. Valgfrie faktiske desktop-/server-/web-artefakter samles og testes kun
med `--help`; det er ikke installeret GUI-accept. De resterende desktopprøver
er efter brugerens ønske samlet til allersidst.
