# Drift af opgaver og påmindelser

## Lagring, adgang og migration

Workspace-control-migration 34 opretter append-only events for opgaver, boards,
rutiner, operationskvitteringer og leverede påmindelser. Aktuel tilstand læses
fra views; historik og effekt gemmes i samme SQLite-transaktion. Databasen er
privat i workspacets `.rentemester/`. Hver juridisk enheds ledger er separat.

Selskabsroller owner, bookkeeper og reviewer kan håndtere opgaver; reader læser.
Owner/bookkeeper administrerer selskabets boards/rutiner. Fælles opgaver med
selskaber kræver adgang til dem alle. Workspaceejere administrerer rent
workspacearbejde; ejerskab giver ikke implicit selskabsadgang. Tildeling er en
ansvarsoplysning og giver ingen rettigheder. Arkiveret arbejde findes via det
synlige arkivfilter og kræver fortsat medlemskab.

Ved opgradering tages en workspace-snapshot med det eksisterende backupflow.
Nye snapshots inkluderer `tasks.json` med historik og kvitteringer; gamle
snapshots uden opgaver understøttes. Restore bevarer dokumentation og nulstiller
ukendte kontoidentiteter i nye events: tildeling fjernes, og deres påmindelser
deaktiveres. Adgangskonti og runtime-leases følger ikke med. Genopret og kontrollér
medlemskab før påmindelser aktiveres. Et ældre image må ikke bruges mod en
workspace-database med en nyere migration; rollback kræver en verificeret backup.

## HTTP, CLI og MCP

Browserens `/opgaver` og `/companies/:slug/opgaver` bruger samme domæne som
`tasks`, `task-series` og `task-boards` i CLI samt de tilsvarende MCP-værktøjer.
Læsninger ændrer ikke tilstand. Mutationer kræver actor, idempotencyKey og ved
ændringer expectedVersion. HTTP udleder actor fra sessionen; MCP kræver confirm.
CLI følger workspace-/selskabspolicyens actor-allowlist, ikke klientens påstand
om rolle. `task-boards save` kræver desuden `--confirm yes`.

Eksempel på manuelt opgaveinput (gem som JSON i eget workspace):

```json
{"title":"Afklar månedsdokumentation","scope":{"kind":"company","companySlug":"example-aps"},"idempotencyKey":"monthly-documentation-2026-10"}
```

```sh
rentemester tasks create --workspace /path/to/workspace --actor agent:approved --input create-task.json
rentemester tasks list --workspace /path/to/workspace --companies example-aps
rentemester tasks get --workspace /path/to/workspace --task-id task-reference
rentemester tasks runtime --workspace /path/to/workspace
```

Actor og selskabsnavn i eksemplet skal svare til lokal policy/registrering.
Se discovery for operationernes strikte inputskemaer. Serie/board gemmes første
gang med expectedVersion 0. Ved 409/version_conflict læses aktuel version,
mens brugerens felter bevares. Ved ukendt resultat læses browserens
`GET /api/tasks/operations/:idempotencyKey`, eller agenten gentager identisk
kald med samme nøgle. En anden payload kræver en ny nøgle. En manglende kvittering
beviser ikke, at en igangværende handling aldrig bliver udført.

## Runtime og påmindelser

`rentemester serve` starter minutrunneren i samme proces som serveren og stopper
den ved SIGINT/SIGTERM. Den indhenter relevante planer ved start og kontrollerer
dem hvert minut. Ingen plan er aktiv fra oprettelsen. Brugeren vælger fristvarsel
eller opfølgningsdato, engangs-/daglig levering og eksplicit tidszone; standarden
er Europe/Copenhagen. Kanalen er alene Rentemester.

Runtime genkontrollerer afslutning, frist, aktiv konto og selskabsadgang før hver
levering. Leveringsslots er persistente og unikke. Ændrede frister gør gamle
beskeder irrelevante; en genstart leverer ikke en strøm af historiske dubletter.
En kort SQLite-lease beskytter mod samtidige serverprocesser. Runtime-status
kræver både en faktisk timer i den serverende proces og frisk, vellykket heartbeat;
en tidligere CLI-kørsel kan ikke få en stoppet server til at se aktiv ud.

Andre deployments kan udføre én deterministisk kørsel via:

```json
{"asOfDate":"2026-10-08","idempotencyKey":"task-maintenance-2026-10-08"}
```

```sh
rentemester tasks run --workspace /path/to/workspace --actor agent:approved --input run-tasks.json
```

Dette er en enkelt kørsel, ikke installation af en scheduler. Angiv companySlugs
for begrænset vedligeholdelse; udelad dem kun ved ønsket samlet scope. CLI-kørslen
giver ikke i sig selv permanent aktiv runtime-status. Runtime sender ingen mail,
bogfører intet og starter intet LLM-arbejde.

Ved inaktiv status: kontrollér at den relevante `serve`-proces kører, læs
`tasks runtime`, kontrollér seneste fejl/heartbeat og workspaceadgang, og genstart
den autoriserede server efter afklaring. Gemte planer og historik bevares.

## Kildedækning og myndighedsgrundlag

Synkronisering gennemlæser alle sider af workbench, åbne undtagelser og
registrerede tidligere perioder. Stabile identiteter inkluderer ledgeridentitet,
selskab, kildeart og konkret emne/periode. Ændret indholdshash skaber ingen ny
opgave. Kildebesøg sker via adgangsbeskyttede produktlinks; rå bilagstekst,
CPR og betalingsoplysninger kopieres ikke ind i korttitler.

- Bankarbejde: aktuelle workbench-rækker og autoritativ bankafstemning.
- Undtagelser: åbne undtagelser, også i tidligere/lukkede perioder; bankoverlap
  samles på samme arbejde.
- Periodelukning: registrerede perioder og deres aktuelle readinesskontroller.
- Moms: eksplicit registreret måned, kvartal eller halvår; nul- og refusionsmoms
  kræver også indberetning. Afslutning kræver indberettet periode med reference.
- Godkendelse: seneste bogføringsrevision og dens egne godkendelsesbeviser.
- Årsafslutning: selskabets regnskabsår. Årsrapportpligt, aktivitet og frister
  uden tilstrækkelig klassifikation fremstår som afklaringsforslag.

Momsfristerne er verificeret mod [Skattestyrelsens officielle fristoversigt](https://skat.dk/erhverv/moms/frister-indberet-og-betal-moms)
den 8. oktober 2026, inklusive månedsmomsens særlige sommerfrist. Reglen er
produktets eksisterende ML §57-beregning. Andre myndighedsfrister beregnes kun
med særskilt kendt grundlag; produktet giver ikke automatisk fuld juridisk
fristdækning. Regler ændres ikke automatisk efter en myndighedsændring.

Et forsvundet eller utilgængeligt grundlag giver unknown og synligt
afklaringsbehov. Det er ikke bevis for, at arbejdet er færdigt. Delvis synk viser
fejl pr. selskab og bevarer gammelt åbent arbejde. Kendte problemer opdateres
idempotent; fortolkningsforslag kræver eksplicit relevansvurdering. Gentagelse af
et løst problem genåbner kortet med bevaret afslutningshistorik.

Et uploadet eksternt bevis er user_reported. Kun en frisk kontrol af den
autoritative produktkilde kan give product_verified. Eksternt indberettet moms
dokumenteres på momsperiodens kildeside med kvitteringsreference; opgaven kan
ikke selv ændre periodens regnskabsmæssige status. Begrundet undtagelse er et
separat afslutningsudfald, ikke skjult kildeafslutning.

## Verifikation og kandidat

[Acceptmatrixen](tasks-implementation.md) binder A01–A36 til tests og skelner
automatik fra menneskelig brugerobservation. `bun run verify:local` er den
samlede lokale gate; [releaseflowet](release/README.md) bygger og verificerer
releasekandidaten på præcis main-commit. Produktion følger fortsat den gældende
Digisense-/promotionsgate.
