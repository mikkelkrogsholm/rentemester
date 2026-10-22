## Bindende styring af Rust-omskrivningen

Målet er hele Rust-omskrivningen i epic #672. Én fælles Rust-motor og server
betjener web, desktop med lokal server og desktop med ekstern server.
Desktop er Rust/Tauri med det eksisterende React-UI; byg ikke parallelle
bogføringsmotorer eller separate desktopskærme. Den manuelle installerede
desktopaccept udføres til allersidst. Bed ikke om skærmoplåsning undervejs.

- SGD ejer pilotens lokale udførelsestilstand; GDD beskriver kontrakter, og
  Graphify beskriver observeret kode. Hooks læser SGD_WORK_PACKAGE og kræver
  fortsat én gyldig Work-Issue-henvisning. Opret ikke en ekstra lokal opgavekø.
- GitHub-issues er autoritative for leverancestatus. Brug præcis én `flow:`-status
  fra [arbejdsgangen](docs/development-workflow.md). `progress.json` gemmer
  evidens og genoptagelse, aldrig en konkurrerende kø. Bevar hele oprindeligt
  aftalte scope og allerede verificeret arbejde.
- Hovedagenten ejer prioritering, fælles kontrakter, integration og accept.
  Underagenter får afgrænsede leverancer med issue, filansvar, afhængigheder og
  færdigkriterium. De må ikke delegere videre eller lukke hovedissuet.
- Hav højst ét fælles brugerforløb aktivt. Udvikl dets uafhængige dele parallelt
  med nyttige underagenter i isolerede worktrees; fælles checkout er read-only
  for underagenter. Overhold plads- og worktreegrænserne nedenfor.
- Hver opgave skal opfylde et konkret acceptkriterium eller fjerne en påvist
  blokering. Valgfri polish, spekulativ hardening og nye features parkeres;
  de må ikke udvide omskrivningens kritiske vej.
- Kør hurtige målrettede tests først og samlet accept ved færdige forløb.
  Gentag kun ved ændret kode, konkret fejl eller ny viden. Kontrollér en defekt
  tests forventning isoleret, før endnu en dyr samlet prøve startes.
- Når accept består, integrér og fortsæt til næste nødvendige forløb.
  Ingen ekstra perfektioneringsrunde. En testfejl må ikke løses ved at svække
  det aftalte brugerresultat eller en nødvendig regnskabs-/adgangskontrol.
- Før skrivning/commit skal checkouts have en afgrænset arbejdspakke til
  hooks. Workers afleverer kode og konkrete testresultater; hovedagenten
  vurderer den integrerede accept. Manglende bevis er ikke færdig.
- Ved genoptagelse afstemmes faktiske GitHub-statusser, Git-ændringer og levende
  processer før ny dispatch. Gentag ikke allerede udførte effekter.
- Status til brugeren beskriver færdige brugerforløb, konkret restarbejde og
  faktiske blokeringer. Interne test-/agenttællere erstatter ikke fremdrift.

## Artefaktformat

Brug kun flade filer som projekt- og arbejdsleverancer. Opret ikke Excel-,
Word- eller andre Office-filer (`.xlsx`, `.xls`, `.docx`, `.doc`), medmindre
brugeren udtrykkeligt beder om det i den konkrete opgave. Brug i stedet fx
Markdown, YAML, JSON, JSONL, CSV, HTML eller OKF. Tabulær viden bør som
udgangspunkt være CSV/JSONL med en menneskelæselig Markdown- eller HTML-visning.

## CLI-kontrakt for agenter

Før du kalder `rentemester`-CLI'en muterende, læs `docs/cli-contract.md`. Kort:

- **Actor-politik**: enhver muterende kommando kræver en actor — `--actor
  <user:...|agent:...|system:...>` (skal stå i `config/policy.yaml`), eller en
  `USER`/`LOGNAME`/`RENTEMESTER_AGENT`/`OPENCLAW_AGENT` miljøvariabel. Uden
  actor afvises kommandoen med `actor required for mutations`.
- **Exit-koder**: `0` = succes (`ok:true`); `2` = parse-/brugsfejl (forkert
  kald — ret flag/argumenter); `1` = forretnings-/ledger-afvisning (kaldet var
  korrekt, men resultatet er `ok:false` — læs `errors[]`).

## Produktgrænse: generel motor, lokale virksomhedsdata

Rentemester er et generelt bogføringsprodukt, som bruges af flere virksomheder.
Produktkode, schema, regler, importere, validering, rapporter og tests skal derfor
være genanvendelige og må ikke indeholde særlogik for en bestemt virksomhed,
et bestemt CVR-nummer, en konkret kontoplan eller et konkret beløb.

- Implementér generelle domænemodeller og konfigurerbare mekanismer. Eksempler er
  Dinero-import, moms-roll-forward, en effektivt dateret virksomhedsgraf,
  mellemregningsafstemning og konsolideringsregler.
- Hold konkrete selskabsnavne, CVR-numre, ejerandele, kontomappinger,
  bankkonti, saldi, bilag og lokale policyvalg i det enkelte workspace eller
  virksomhedens konfiguration — aldrig som defaults eller hardcoding i
  GitHub-koden.
- Tests for generel kode skal bruge syntetiske virksomheder og beløb og dække
  både positive og fail-closed scenarier. En reel virksomheds eksport kan være
  et lokalt acceptkorpus, men må ikke checkes ind eller blive en skjult
  forudsætning for produktlogikken.
- En generel importregel skal udlede sin beslutning af dokumenterede
  kildefelter og regnskabsmæssige invariants. Hvis en bestemt virksomheds data
  kræver mapping eller menneskelig vurdering, gemmes beslutningen lokalt med
  auditspor; kontrollen må ikke svækkes globalt.
- En virksomhedsgraf i produktet beskriver det generelle schema og de generelle
  operationer. Den konkrete graf-instans tilhører workspacet. Hver juridisk
  enhed beholder sin egen ledger; koncernrapportering ligger som et
  dokumenteret read-only lag med eksplicitte elimineringer.

## graphify

I SGD-workers foretages den eksplicitte AST-opdatering før aflevering med
`graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"`.
Det holder genererede observationer ude af workers' kodeområder og ændrer ikke
hovedgrafens semantiske indhold. Hovedagenten kører `graphify update .` på den
integrerede kode før publicering og opdaterer ændrede dokumenters semantik.


This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the Graphify CLI before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).

## Diskplads, worktrees og Rust-bygninger

- Brug højst fire aktive agent-worktrees. Genbrug et egnet checkout, og fjern
  færdigintegrerede worktrees efter hver integration. Bevar ikke-integrerede
  commits og ufærdige ændringer i verificerede, gendannelige Git-snapshots før
  fjernelse. Brug Codex' worktree-arkivering, hvor den er tilgængelig.
- Uafhængigt arbejde og Rust-bygninger må køre parallelt. SGD giver hver worker
  sit eget absolutte `CARGO_TARGET_DIR` og konfigurerer `CARGO_BUILD_JOBS` ud fra
  reelle ressourcer. Tving ikke workers ind i én target-mappe eller global kø.
  Uden eksplicit target kan hovedcheckoutets eksisterende cache genbruges.
- Projektets `scripts/rust/cargo-guard.sh` og Python-kontrol skal fortsat afvise
  kompilering under **30 GiB ledigt**, med **15 GiB** som kritisk grænse, på både
  kilde- og byggevolumen. Bevar outputvalidering og eksisterende data. En ændret
  byggeproces er ikke tilladelse til at fjerne plads- eller regnskabskontroller.
- Kør `scripts/rust/cargo-guard.py status` før større bygninger og nye parallelle
  arbejdsrunder. Brug `scripts/rust/cargo-guard.py cargo <kommando>` til eksplicit
  preflight, også når Cargo kan genbruge alle artefakter. Incremental er slået fra.
- Fjern færdigintegrerede worktrees og genopbyggelige worker-output efter aflevering;
  bevar først snapshots af ikke-integreret arbejde. Rør aldrig virksomhedsdata,
  testrapporter eller ufærdig kode som automatisk oprydning. `cargo clean` må kun
  rydde den valgte byggecache, når den ikke bruges af en levende bygning.
- I containere gælder de samme pladsgrænser. Der må være separate bygge-output
  pr. worker; dependency-downloadcache kan deles. Ingen global compilerlås.

Den fastlåste Linux-byggecontainer har ikke Python. Her håndhæver shell-launcheren
samme output- og pladsgrænser på både byggevolumen og den delte kildemappe.
Lokale værtsmaskiner bruger Python 3; en manglende kontrol må aldrig betyde,
at kompilering fortsætter uden pladsgrænser.
