# Opgaver: implementering og accept

Status: implementeret til releasekandidat 0.3.0. Automatisk verifikation er
beskrevet nedenfor. Den repræsentative menneskelige brugertest af ét-minutsmålet
afventer observation; den er ikke erklæret bestået.

## Kontrakt

Ét workspace-ejet opgavegrundlag med selskabs- og workspace-scope, fælles
status og lokale boards. Liste, kalender, kanban og årshjul bruger samme
referencer. Opgavearbejde ændrer ingen ledger, betaling eller indberetning.
Første komplette levering omfatter hele den aftalte oplevelse. Påmindelser
er brugeraktiverede og leveres inde i Rentemester; ingen mail eller LLM-runner.

Produktkode og tests bruger syntetiske data. Private virksomhedsdokumenter
kopieres ikke ind i repository, issues eller PR'er. Ekstern kalenderintegration,
ressourceplanlægning og automatisk juridisk vedligeholdelse er uden for opgaven.

## Implementeringsgrænse

Workspace-migration og core-opgaver/boards; kildeadaptere, serier og påmindelser;
HTTP, CLI og MCP; fire cockpit-visninger; snapshot/restore, adgang og runtime.
Eksisterende kildesider og deres kontrakter bevares.

## Aflevering

Acceptmatrix A01–A36 med konkrete testreferencer, samlet syntetisk browserforløb,
adversarial review og fuld lokal verifikation. Den repræsentative brugertest
med ét-minutsmålet kræver en faktisk brugerobservation og må ikke erstattes
af en påstand baseret på screenshots eller automatiske tests.

Kode og dokumenter opdateres i Graphify før commits og aflevering. En kandidat
bygges gennem eksisterende releaseflow; produktionspromotion følger de gældende
godkendelser. Se [driftsvejledning](tasks-operations.md),
[CLI-kontrakt](cli-contract.md#5-opgaver-rutineserier-og-boards) og
[MCP-overflade](mcp-tool-surface.md).

## Acceptmatrix A01–A36

Testhenvisningerne nedenfor er repository-relative. Core-tests kører mod rigtig
SQLite og syntetiske workspaces. HTTP-tests bruger den rigtige router og
adgangskontrol. Komponenttests og `tasks-workflows.spec.ts` simulerer API-svar;
de beviser UI-adfærd, ikke serverintegration. `tasks-live.spec.ts` starter en
rigtig Bun-server, ledgers, workspace-database og minutruntime uden API-mocks;
hele forløbet køres i Chromium og WebKit samt 320 px mobilvisning.

Forkortelser: **D** = `tests/unit/tasks.test.ts`; **B** =
`tests/unit/task-boards.test.ts`; **S** = `tests/unit/task-series.test.ts`;
**K** = `tests/unit/task-sources.test.ts`; **R** =
`tests/unit/task-reminders.test.ts`; **A** = `tests/unit/task-service.test.ts`;
**H** = `tests/unit/server-api/tasks.test.ts`; **I** =
`tests/unit/task-agent-interface-contract.test.ts`; **UI** =
`app/src/views/TasksView.test.tsx` (inklusive formulartests); **Browser** =
`app/e2e/tasks-workflows.spec.ts`; **Live** = `app/e2e/tasks-live.spec.ts`.

| ID | Acceptgrundlag | Automatisk evidens |
| --- | --- | --- |
| A01 | Samme reference i selskab og samlet | H: HTTP-livscyklus; Live: oprettelse samlet og læsning i Green ApS |
| A02 | Status deles mellem visninger | B: fælles/lokal placering; UI: alle fire visninger; Live: Hos revisor → Afventer i samlet detalje |
| A03 | Scope vælges synligt ved samlet oprettelse | UI og Browser: gem blokeret indtil eksplicit scope; H/I: scope kræves |
| A04 | Filtre anvendes efter adgang | A: søgning/counts uden skjult selskab; UI: URL-filtre og tilbagevej; Browser: selskabsfiltreret kanban |
| A05 | Fælles A+B tælles én gang | A: common A+B-test; D: tælling én gang; Live: fælles møde; K/A: ingen autoritative ledger-effekter |
| A06 | Hos revisor betyder Afventer samlet | B: fælles status med lokal placering; Browser og Live: opret og flyt til Hos revisor |
| A07 | Tvetydig flytning kræver valg | B: `workspace movement must choose an ambiguous company column`; UI: synlig flyttevælger |
| A08 | Kolonnesletning kræver omplacering | B: atomisk relocation og bevaret bevis; UI: konsekvensvisning og destinationsvalg |
| A09 | Omdøbning ændrer ikke status | B: rename/reorder bevarer status og version; A: hash uafhængig af skjult arbejde |
| A10 | Ny statusbetydning vises før anvendelse | B: aktuelt previewhash og afvisning af stale preview; UI: preview før gem |
| A11 | Arbejdsdato ændrer ikke myndighedsfrist | D: dokumenteret ændringsgrundlag kræves; R: flyttet arbejdsdato stopper ikke fristvarsel |
| A12 | To forskellige datoer, ét kort | D: count én gang; Browser og Live: arbejdsdato/fristen vises i agenda |
| A13 | Udateret/utildelt arbejde kan oprettes og findes | D: minimal oprettelse og filtre; UI: eksplicit scope ved oprettelse |
| A14 | Gammel og aktuel rutine er særskilte | S: uafsluttet gammel og aktuel periode med selvstændige identiteter og statusser |
| A15 | Serieændring/pause bevarer udestående og historik | S: future-only edits, ændret kadence og pause; UI: eksplicit fremtidsformular |
| A16 | Ikke relevant er et begrundet separat udfald | D: exception/non-applicability adskilt fra completed; UI: udfald og note |
| A17 | Ukendt aktivitet vises som afklaringsbehov | S: activity relevance forbliver unknown; D: flag kan ikke slettes uden grundlag |
| A18 | Egne momsperioder og regnskabsår respekteres | K: måned/kvartal, nul-/refusionsmoms og forskudt år; S: fiskale kvartaler og egne ankre |
| A19 | Forventet næste år adskilles fra konkrete kort | S: fremtidsprojektion uden writes/implicit sikker frist; UI/Browser: forventede forekomster og synligt startårsfelt |
| A20 | Kildesynkronisering skaber ikke dubletter | K: mere end 100 workbench-rækker, overlap, ændret hash og gentagen synk; D: stabil kildeidentitet |
| A21 | Kortflyt løser ikke manglende bilag | K: reel bankkilde kontrolleres; H/D: done-status kan ikke omgå dokumenteret afslutning |
| A22 | Genopstået kildeproblem genåbner med historik | K: løst bankarbejde og ny undtagelse; D: reappearing source med bevaret bevis |
| A23 | Manglende afslutningsbevis afvises | D/K: krævet, frisk kildebevis; Browser: note alene afvises; Live: kvittering gemmes |
| A24 | Ekstern revisor kan dokumenteres med kvittering | D: opaque external_receipt og user_reported assurance; Browser/Live: manuel kvittering + note; K: separat test af indberettet momsperiodes referencekrav |
| A25 | Genåbning bevarer afslutning uden tilbageførsel | D/H/Browser: genåbning med årsag og historik; A/K: ledger uændret |
| A26 | Klar til godkendelse udfører ingen regnskabshandling | B: statusflytning/board kan ikke afslutte eller genåbne uden domænehandling; K: seneste batchrevision læses, aldrig godkendes/anvendes |
| A27 | Uklar ekstern handling kræver verifikation | D: unknown kan ikke blot nulstilles; UI: referencer, relevant vurdering og forklaring kræves; Browser: uklart mutationssvar verificeres med samme nøgle |
| A28 | Ekstern ansvarlig får ingen adgang/mail | A: tildeling giver ingen selskabsret; R: ekstern uden konto får ingen levering |
| A29 | Aktiv påmindelse ophører efter afslutning | R: opt-in, restart, DST, fristændring og completion; A: faktisk runtime og mistet medlemskab; Live: aktiv/inaktiv levering |
| A30 | Ingen læk af titler/counts/referencer/beskeder | A: adgang før søgning, historik, kvitteringer, bulk og preview; I: afvist credential kan ikke forfalske actor; protected knowledge HTTP-test: scoped bodies |
| A31 | Arkiveret selskabs arbejde bevares | A: særskilt includeArchived, retained membership og åben status |
| A32 | Samtidige writes overskriver ikke skjult | D: to SQLite-forbindelser; H: 409; UI: brugerfelter og versionskonflikt bevares; B: previewkonflikter |
| A33 | CPR og fulde bankoplysninger kopieres ikke til kort | K: generiske kildetitler uden kildetekst; D: genkendelige private data afvises i titel, ansvarlig og note; R: beskeder bruger korttitlen |
| A34 | Gentagelse genfinder eksisterende effekt | D/H: principal-specifik kvittering, payloadkonflikt og historik; I: rigtig CLI + MCP over samme database; Browser: ukendt resultat verificeres uden nyt POST |
| A35 | Mobil og tastatur kræver ikke drag-and-drop | Browser: 320 px, fokus, Tab/Enter, flyttevælger og axe; Live: mobilagenda/reflow i Chromium/WebKit |
| A36 | Manglende grundlag giver synlig usikkerhed | K: manglende ledger/ændret ledgeridentitet; S: ukendt relevans; Browser: første fejlede synk med tom liste viser ufuldstændigt grundlag |

Matrixen dokumenterer konkrete automatiske scenarier. Den er ikke en påstand om
manuel observation af hver kombination af rolle, enhed, dato og browser.
Snapshot/restore har yderligere 13 specifikke tests i
`tests/unit/task-snapshot.test.ts` samt eksisterende workspace-snapshottests:
historik, idempotens, ukendte identiteter, checksum, struktur, scope, bulk-receipts
og ældre snapshots. Almindelige opgavemutationer ændrer ikke ledgerbytes.

## Review og kendte grænser

Adversarial review reproducerede og fik regressionstests for adgangstab ved
kvitteringsgenbrug, historisk scope, projektioner med ændret scope, workspace-
actorpolitik, boardets fejlsvar og deterministiske previewhash. Korte gyldige
playbook-slugs og runtime-status gennem read-only snapshots blev også rettet.

Genkendelige CPR-/IBAN-/bankkontomønstre afvises i opgavens fritekst. Det er et
værn mod kopiering af identificerbare betalingsdata, ikke en garanti for at
genkende enhver omskrivning. Beskyttede dokumentreferencer er det tilsigtede
sted at finde detaljer. Se driftsvejledningen for præcis kildedækning.

## Reproducerbar verifikation og brugertest

Kør `bun run verify:local` for projektets samlede gate, og
`bun run version:check` for identiteten. Det inkluderer de nye domæne-, HTTP-,
agent- og browserintegrationstests. Testdata er syntetiske og kortlivede.

Til en faktisk brugerobservation: byg cockpittet og start
`bun scripts/tasks-live-server.ts`. Åbn den returnerede URL med `/opgaver`.
Bed en repræsentativ ejer/bogholder finde månedens vigtigste arbejde på tværs
af selskaberne inden for ét minut uden instruktion i navigation. Registrér
rolle, varighed, fundne opgaver og eventuelle problemer. Stop med Ctrl-C;
workspace og runtime fjernes. Automatiske browsertests erstatter ikke dette.

Observationsstatus ved implementeringen: afventer menneskelig tilbagemelding.
