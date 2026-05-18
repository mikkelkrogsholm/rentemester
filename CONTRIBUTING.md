# Sådan bidrager du til Rentemester

Tak fordi du vil hjælpe. Rentemester er et open source bogholderisystem til danske mikrovirksomheder. Det betyder at hvert eneste bidrag potentielt rammer en virksomheds rigtige regnskab — så vi har en lav tærskel for at sige hej, men en høj tærskel for hvad der lander på `main`.

Det vigtigste først: **læs [docs/build-loop.md](docs/build-loop.md) inden du går i gang**. Det er kontrakten alle bidrag følger.

---

## TL;DR

1. Find et issue der trækker dig (eller åbn ét med en hypotese).
2. Skriv den fejlende test først — en negativ case.
3. Patch minimalt inden for ét modul.
4. Kør `bun test` + `bun run smoke` lokalt.
5. Åbn PR med én meningsfuld commit. Ref'er issue-nummeret.
6. Vent på CI grønt + review.

---

## Grundprincipper

Tre ting er ikke til forhandling:

1. **Reglerne afgør, ledgeren håndhæver.** En agent eller bruger må gerne foreslå noget, men kerne-API'erne afviser hvis det bryder bogføringsloven, momsloven eller en regel i [`rules/dk/*.yaml`](rules/dk/). Vi skriver ikke "vi stoler på at brugeren" — vi skriver eksplicit validering.

2. **Determinisme er en feature.** Samme input → samme output, bit-for-bit. Det betyder: ingen `Date.now()` i forretningslogik (brug eksplicitte `asOfDate`-parametre), ingen `Math.random()`, ingen tilfældig nøglerækkefølge i JSON-eksport. Hvis du har brug for "current time", tag den ind som parameter.

3. **Audit-spor er hellig.** `journal_entries` er append-only via SQLite-trigger. Rettelser sker via reverseringer, ikke UPDATE. Det samme gælder bilag, fakturaer, betalinger, rykkere osv. Hvis du har brug for at "rette" noget, append en korrektion — ingen undtagelser.

---

## Det første bidrag

Den letteste vej ind:

- Issues mærket [`good first issue`](https://github.com/mikkelkrogsholm/rentemester/labels/good%20first%20issue) er gode at starte med.
- Eller pluk et issue fra det åbne backlog — vi har både små opgaver (et CLI-flag, en valideringsregel) og store epics (MCP-server, dashboard).

Hvis du ikke ved hvor du skal starte: åbn et issue med spørgsmål. Vi svarer.

---

## Build-loop-kontrakten

Hver ændring følger samme cyklus. Forkortet fra [docs/build-loop.md](docs/build-loop.md):

### 1. Vælg et mål

Skriv din hypotese i én sætning. Eksempel: `dansk bank-CSV med ; separator fejler ved import`.

### 2. Klassificér

- **bugfix** — ødelagt adfærd → kan gå til `main` efter grønne gates
- **hardening** — bedre validering eller audit → kort branch hvis multi-step
- **feature** — ny capability → kræver acceptance-fixture og må ikke have skjulte ledger-side-effekter
- **research** — ingen `main`-commits; dokumentér i issue eller branch

### 3. Skriv den fejlende test først

Tilføj en unit-, CLI- eller smoke-regression der fejler på den nuværende kode. For accounting-adfærd: inkluder mindst én **negativ case** — hvad skal blokkeres.

### 4. Patch minimalt

- Hold dig inden for det mindste modul- eller core-grænse.
- Bland ikke refactor med rule-ændringer medmindre refactoren kræves for fixet.
- Brug deterministiske identifikatorer i fixtures og smoke-flows.

### 5. Verificér i gates

```bash
bun test path/to/your.test.ts    # din test først
bun test                          # alt grønt
bun run smoke                     # frisk /tmp/rentemester-smoke
git diff --check                  # ingen trailing whitespace
```

### 6. Review din diff før commit

- Ingen genereret company-data
- Ingen secrets
- Ingen temp-filer
- Ingen halv-byggede features
- Regler, docs og tests matcher din adfærd

### 7. Commit og PR

- Én meningsfuld commit pr. loop (squash hvis nødvendigt).
- Imperativ besked: `Harden bank CSV semicolon delimiter`, ikke `WIP` eller `fixes stuff`.
- Push kun grønt. Hvis din loop ikke er færdig: park på en branch med noter.
- Åbn PR mod `main`. Ref'er issue med `Closes #N` hvis fixet løser et issue.

---

## Code style

- **TypeScript strict.** Vi bruger Bun's indbyggede transpiler; ingen Babel.
- **Funktioner over klasser** medmindre du har en stærk grund.
- **Ingen kommentarer der gentager koden.** Skriv kun WHY — fx en lovhenvisning, et historisk incident, eller en subtil invariant.
- **Beløb i øre.** Brug `src/core/money.ts` for al valuta-aritmetik. Float-aritmetik på beløb er et bug.
- **Datoer i ISO.** `YYYY-MM-DD` for forretningsdatoer, `YYYY-MM-DDTHH:mm:ss.sssZ` for timestamps. Brug `src/core/dates.ts`.

---

## Tests

Vi har én test-stil: **unit-tests pr. modul** + **CLI-tests pr. kommando** + **smoke-test** der dækker hele pipelinen.

- Filnavngivning: `tests/unit/<modulnavn>.test.ts` for core-logik, `tests/unit/<modul>-cli.test.ts` for CLI.
- Brug `bun:test` (`describe`, `test`, `expect`).
- Test både happy path og **mindst én negativ case** for accounting-adfærd.
- Smoke-test bruges som regression-net for hele lifecyclen — udvid den når en ny CLI-kommando indfases.

---

## Skema-ændringer

Hvis du tilføjer eller ændrer en tabel i [`src/core/schema.sql`](src/core/schema.sql):

1. Tilføj kolonner med `ALTER TABLE` i [`src/core/db.ts`'s `migrate()`](src/core/db.ts) — så eksisterende databaser kan opgraderes.
2. Tilføj append-only-triggers hvor det giver mening (alle finansielle tabeller).
3. Inkludér en mini-test der verificerer migrationen mod en eksisterende SQLite-fil med den gamle schema.

---

## Regel-ændringer

Hvis du ændrer noget i [`rules/dk/*.yaml`](rules/dk/):

1. Bump `version`-feltet i bunden af YAML'en.
2. Tilføj et `rule_id` der følger mønstret `DK-<KATEGORI>-<NAVN>-<NUMMER>`.
3. Reference en konkret kilde i [`sources/legal-sources.json`](sources/legal-sources.json) via `source_id`.
4. Hvis kilden ikke er der: kør `bun run sources:download` og commit den downloadede XML/HTML med SHA-256 i index.json.
5. Tilføj en test der verificerer at koden faktisk håndhæver reglen.

---

## Spørg om hjælp

- **Hvis du er usikker på en juridisk fortolkning**: åbn et issue med tag `question` og link til den lovparagraf du fortolker. Det er bedre at diskutere end at gætte.
- **Hvis din PR sidder fast i review**: skriv et comment. Vi prøver at svare inden for et par dage.
- **Hvis du finder en sikkerhedsfejl**: åbn ikke et offentligt issue. Skriv direkte til <mikkel@56n.dk>.

---

## Kode af AI-agenter

Rentemester er agent-first — det er forventet at en del af bidragene kommer fra AI-agenter (Claude, Cursor, Codex osv.).

Hvis du lader en agent bidrage:

- **Du** er forfatteren, ikke agenten. Du har ansvaret for at PR'en er korrekt.
- **Agent-commits markeres** med trailer-line `Co-Authored-By: <agent name> <noreply@anthropic.com>` (eller tilsvarende).
- **Agenten skal følge build-loop-kontrakten** — failing test først, minimal patch, grønne gates. Hvis du bare lader agenten "fixe det", bliver PR'en afvist.

Vi har ingen problem med agent-bidrag, vi har et problem med dårlige bidrag.

---

## Licens

Ved at åbne en PR accepterer du at dit bidrag licenseres under [MIT](LICENSE).

---

Velkommen indenfor. Bogføring fortjener bedre.
