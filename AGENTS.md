## Bindende styring af Rust-omskrivningen

Målet er hele Rust-omskrivningen i epic #672. Én fælles Rust-motor og server
betjener web, desktop med lokal server og desktop med ekstern server.
Desktop er Rust/Tauri med det eksisterende React-UI; byg ikke parallelle
bogføringsmotorer eller separate desktopskærme. Den manuelle installerede
desktopaccept udføres til allersidst. Bed ikke om skærmoplåsning undervejs.

- GitHub-issues er autoritative for opgavestatus. Brug præcis én `flow:`-status
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
- Alle lokale Rust-bygninger deler hovedcheckoutets `target/`. Projektets
  `.cargo/config.toml` aktiverer `scripts/rust/cargo-guard.sh`, som bruger
  Python-kontrollen til at forbinde worktrees med cachen og afvise separate
  compiler-outputmapper. Opret
  aldrig selvstændige `target`-mapper for agenter, tests eller featuregrene.
- Kør `scripts/rust/cargo-guard.py status` før større bygninger og nye parallelle
  arbejdsrunder. Cargo-kontrollen afviser kompilering under **30 GiB ledigt**;
  under **15 GiB** markeres pladsmanglen som kritisk. Ryd op før nyt arbejde.
  Kontrollen kører også ved almindelige `cargo build`, `check`, `test` og
  `clippy`; fjern eller omgå ikke wrapperen eller dens grænser.
- Brug `scripts/rust/cargo-guard.py cargo <kommando>` til eksplicit preflight,
  også når Cargo ellers kan genbruge alle artefakter. Rust-bygninger kører
  med én compilerproces og uden incremental-cache. Cargo serialiserer adgang
  til den fælles cache; parallelisér udvikling, ikke konkurrerende bygninger.
- Ryd først genopbyggelige byggefiler og afsluttede worktrees.
  `scripts/rust/cargo-guard.py cargo clean` må bruges til den fælles byggecache,
  også under pladsgrænsen, når ingen bygning kører. Slet aldrig virksomhedsdata,
  testrapporter eller ufærdig kode som automatisk oprydning.
- Isolerede containere bruger én fælles monteret cache pr. byggemiljø. Kun
  runnerens operatørkonfiguration må sætte `RENTEMESTER_CARGO_CACHE`;
  `CARGO_TARGET_DIR` skal pege på samme mappe. Lokale agenter må ikke bruge
  dette til at omgå den fælles cache.

Den fastlåste Linux-byggecontainer har ikke Python. Her håndhæver shell-launcheren
samme cache- og pladsgrænser på både byggevolumen og den delte kildemappe.
Lokale værtsmaskiner bruger Python 3; en manglende kontrol må aldrig betyde,
at kompilering fortsætter uden pladsgrænser.
