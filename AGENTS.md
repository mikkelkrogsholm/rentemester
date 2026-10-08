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

**Graphify skal altid benyttes først ved arbejde med Rentemesters kodebase.**
Det gælder spørgsmål, analyse, udvikling, debugging, review, adversarial review
og refaktorering, også når opgaven delegeres til subagenter. Brugeren behøver
ikke skrive `/graphify` for at aktivere dette krav.

- Kør først `graphify query "<opgavens spørgsmål>"` fra det checkout, hvor
  arbejdet udføres, før rå kodesøgning eller kodegennemgang. Grafen ligger i
  `graphify-out/graph.json`. Brug `graphify path "<A>" "<B>"` til relationer og
  `graphify explain "<begreb>"` til fokuserede begreber. Kontrollér derefter
  relevante fund i kildekoden; grafen erstatter ikke verifikation.
- Når brugeren skriver `/graphify`, skal Graphify-CLI'en benyttes før andet
  arbejde. Hvis grafen mangler, bruges Graphify-skillen til at etablere den.
  Ved en konkret værktøjsfejl skal begrænsningen oplyses; almindelig
  kodesøgning må ikke beskrives som brug af Graphify.
- Fravælg kun Graphify, når brugeren udtrykkeligt beder om det, eller når
  selve opgaven er at reparere forældet eller forkert grafoutput. Dirty filer
  i `graphify-out/` er forventelige og er ikke en undtagelse.
- Brug `graphify-out/wiki/index.md` til bred navigation, hvis filen findes.
  Læs kun `graphify-out/GRAPH_REPORT.md` ved bred arkitekturgennemgang, eller
  når `query`, `path` og `explain` ikke giver tilstrækkelig kontekst.
- Efter kodeændringer skal `graphify update .` køres fra det aktive checkout
  før commit og før opgaven afsluttes. Opdateringen er AST-baseret og bruger
  ingen LLM. En read-only subagent overlader opdateringen til hovedagenten.
- Ændrede dokumenter, regler og billeder kræver også semantisk opdatering
  gennem Graphify-skillen. Bevar eksisterende semantisk indhold, og registrér
  eventuelle uopdaterede kilder ærligt. En AST-opdatering må ikke rapporteres
  som en semantisk opdatering.
- Git-hookene `post-commit` og `post-checkout` vedligeholder grafen i
  hovedcheckoutet. Graphifys Git-hooks springer linked worktrees over;
  agentens eksplicitte opdateringskrav gælder derfor også i worktrees.
  `graphify hook status` kontrollerer installationen, men beviser ikke, at
  en opdatering er gennemført. Se `docs/graphify.md` for opsætning og kontrol.
