# Rust-omskrivningens arbejdsgang

GitHub-issues under [epic #672](https://github.com/mikkelkrogsholm/rentemester/issues/672)
er autoritative for leverancestatus. Bevar de eksisterende 55 issues, deres fulde
acceptkriterier og afhængigheder. Der oprettes ikke et nyt parallelt board.

Den lokale udførelse bruger State Graph Development: GDD beskriver kontrakter,
Graphify giver kodeobservationer, og Go-værktøjet ejer `.sgd/state.json` for den
aktuelle afgrænsede leverance. Docker Agent-workers arbejder i egne worktrees.
Dette er ikke en kopi af produktbackloggen. Hovedagenten reviewer faktisk kode,
kontrollerer integreret accept og opdaterer først derefter GitHub-leverancestatus.

Uafhængig udvikling og Rust-bygninger må køre parallelt med separate absolutte
`CARGO_TARGET_DIR` og ressourcebestemte `CARGO_BUILD_JOBS`. Der er ingen global
compilerkø. Byggewrapperen bevarer 30/15 GiB-grænserne på kilde- og byggevolumen,
outputvalidering og eksisterende data. Brug aftalte checks med faktisk tilgængelige
programmer; projektets `scripts/rust/cargo-guard.py cargo ...` finder den installerede
Cargo også når den ikke står på runnerens `PATH`. En fejlet check-opstart er ikke
et fejlet Rust-testresultat. Bevar fejl og afleveret kode; gentag ikke modelarbejdet
for at rette en testkommando.

```text
flow:backlog → flow:ready → flow:developing → flow:integrating → flow:acceptance → flow:done
                                    ↑                              │
                                    └──── konkret påvist fejl ──────┘
```

Tilstandene har følgende fokusgrænser. Review og publicering er eksisterende
SGD-trin inden for GitHub-forløbet; der indføres ingen nye statuslabels.

| Tilstand | Tilladt arbejde | Guard mod overengineering og betingelse for næste trin |
| --- | --- | --- |
| backlog | Bevar scope, leveret kode og gyldig evidens. Vælg det nærmeste issue, der kan lukkes helt. | Ingen implementering eller bred ny analyse af hele backloggen. Vælg ud fra konkrete manglende acceptpunkter. |
| ready | Afklar **hele det valgte issues restaccept**, nødvendige afhængigheder, filansvar og mindste tilstrækkelige checks. | Genbrug eksisterende kontrakter og bevis. Ingen ny platform, rapporteringsrunde eller valgfri features. Stop planlægningen, når arbejdet kan udføres og accepteres. |
| developing / SGD running | Byg de manglende acceptpunkter; nyttige uafhængige dele kan udvikles parallelt. | Hver ændring skal kunne pege på et konkret acceptpunkt eller en påvist blokering. Ingen oprydning, abstraktion eller hardening uden den forbindelse. Stop udviklingen, når restaccepten er dækket. |
| review / SGD submitted | Review faktisk diff, kontrakt og konkrete checkresultater. | Afvis kun for en konkret mangel i aftalt scope eller nødvendig kontrol. Stilpræferencer og tænkte fremtidige behov må ikke udløse en ny runde. Grønne tests alene er ikke review. |
| integrating / SGD integrated | Integrér godkendt kode og ret konkrete samspilsfejl. | Ingen ombygning af allerede accepterede dele. Bevar resultater og effekter; genstart ikke færdige workers. Gå til accept, når delene hænger sammen. |
| acceptance | Prøv hele issuets oprindelige accept med gyldigt, revisionsbundet bevis. | Genbrug gyldigt bevis; kør kun nye checks for ændringer eller konkrete huller. Ved 15 minutters testopsætning eller to opsætningsfejl stoppes opsætningsrunden og årsagen isoleres. Ingen svækkelse af regnskabs-, adgangs- eller datakontroller. Bestået accept afslutter forbedringsarbejdet. |
| publicering / SGD publish | Publicér accepteret kode, registrér bevis og luk det samme GitHub-issue. | Ingen ekstra test-, polish- eller dokumentationsrunde uden et påvist acceptkrav. Lokal SGD `done` er ikke GitHub-lukning. Manglende restaccept betyder tilbage til samme issue. |
| done | GitHub bekræfter closed og flow:done. Vælg derefter næste nødvendige issue. | Ingen ekstra perfektioneringsrunde. Genåbn kun ved konkret påvist fejl. |
| blocked / brugerstop | Bevar kode, kvitteringer, fejl og præcis restaccept. | Ingen blind retry eller nyt navn til samme forsøg. Afklar én konkret årsag. Brugerstop er bindende; planændringer genstarter ikke arbejdet. |

Der skal være præcis én statuslabel pr. issue. `flow:blocked` er en ekstra
markering med en selvstændig linje `Flow-Blocked: konkret årsag` i issue-body;
den erstatter ikke status. Fjern markeringen og årsagen, når blokeringen er
løst. En genåbning eller retur fra accept kræver konkret begrundelse.

Et SGD-forløb sigter mod at lukke **ét helt eksisterende issue**. Underopgaver
kan være små og parallelle, men delaccept frigiver ikke fokus til et nyt issue.
Hvis et delforløb er accepteret, fortsætter hovedagenten med samme issues
konkrete restaccept uden at genafspille leverede effekter.

Parkering fra en aktiv status til backlog kræver `--interrupt user-stop`,
`dependency` eller `external-blocker` samt begrundelse og bevaret evidens.
De to blokeringstyper kræver også `flow:blocked` og `Flow-Blocked:` i body.
"Komponenten er færdig", almindeligt resterende kodearbejde eller lyst til
et andet spor er ikke blokeringer. Oplys den konkrete uafklarede afhængighed
eller eksterne forudsætning og restaccepten. Brugerstop kræver ikke en opdigtet
blokering. Genoptagelse kræver brugerens mandat og går gennem ready/developing.

Højst ét underissue er i developing/integrating/acceptance ad gangen. Epic'ens
opsamlingsstatus tæller ikke med. Arbejdet i det aktive issue deles i nyttige,
uafhængige leverancer; det må ikke begrænses til én sekventiel agent.

## Ansvar og aflevering

Hovedagenten ejer prioritering, fælles kontrakter, integration og endelig
accept. Underagenter har et issue, afgrænset filansvar, acceptkriterium og
påkrævet testbevis. De skriver i isolerede worktrees, laver ikke nye
agentlag og lukker ikke parentissuet. Samme fil eller fælles schema har én
skriveansvarlig ad gangen. Hovedagenten må foretage integrationens nødvendige
fælles rettelser.

Brug hurtige målrettede tests først. En dyr samlet prøve køres, når forløbet
hænger sammen; gentag kun ved ændringer, konkret fejl eller ny viden.
Accepterede forløb får ikke en ekstra perfektioneringsrunde. En detalje skal
opfylde et nødvendigt acceptkriterium eller løse en konkret blokering for at
blive på den aktive vej. Bevar nødvendige kontroller af bogføring, adgang,
data, genstart og eksterne handlinger.

Et færdigt delforløb lukker ikke automatisk det store domæneissue. Registrér
implementeringsreference, udførte checks, resultater og de konkrete resterende
acceptpunkter. Testkommandoer eller links alene beviser ikke resultatet.
`docs/architecture/rust-target/implementation/progress.json` gemmer
regnskabsevidens og genoptagelse; det er ikke en konkurrerende kø.

Manuel accept af den installerede desktopapp udføres til allersidst.
Udvikling, headless web/server/CLI/MCP-prøver og pakning fortsætter uden
oplåsning af skærmen. Det er én fælles Rust-motor, samme serverkode lokalt og
på serveren og samme React-UI indbygget i Rust/Tauri-desktopappen.

## Kontroller

`scripts/workflow/rewrite.py` giver status, validering og kontrollerede
statusovergange mod de eksisterende GitHub-issues. Overgange kræver en
begrundelse; acceptance kræver en evidensreference, og done kræver den nedenstående
fulde lukningskvittering. Statusændringen
bevarer øvrige labels og skriver et spor i issue-kommentarerne.

Git-hooks kræver en afgrænset arbejdspakke med issue, ejer, tilladte filstier,
accept og checks. `pre-commit` afviser staged ændringer uden for filansvaret;
`commit-msg` kræver `Work-Issue: #NUMMER`, der matcher pakken. I SGD læser hooks
den aktuelle `dispatch.json` via `SGD_WORK_PACKAGE`, `SGD_TASK_ID` og `SGD_PHASE`.
Issue kommer fra opgavens `commit_trailers`; ejerskab og checks kommer fra den
samme dispatch. Der oprettes ingen ekstra opgavestatus. Ved hovedagentens manuelle
arbejde kan pakken fortsat ligge i det enkelte worktrees Git-metadata.
Hooks har ingen netværksafhængighed. Installation bevarer eksisterende
Graphify-hooks og afviser synligt at overskrive andre hooks.

CI-kontrollen er publiceret på `main` gennem
[PR #728](https://github.com/mikkelkrogsholm/rentemester/pull/728).
På pull requests kører den de hurtige workflowtests og kontrollerer
issue-referencer i nye relevante commits; historiske commits omskrives ikke.
Et særskilt job med læserettigheder kontrollerer alle 56 aktuelle GitHub-
statusser ved relevante issueændringer, pull requests, publicering og manuel
workflow-start. Urelaterede issue-events springer statusopslaget over.

Både [publiceringskørslen](https://github.com/mikkelkrogsholm/rentemester/actions/runs/39575219413)
og en [faktisk automatisk issue-kørsel](https://github.com/mikkelkrogsholm/rentemester/actions/runs/39575296716)
består. En almindelig redigering af epic'en udløste den sidste prøve.
Automatikken validerer og rapporterer; den omskriver ikke labels eller lukker
issues. Lokale hooks kræver fortsat installation i hver clone.

Hooks kan omgås med Git, og GitHub-labels kan ændres manuelt. Den samlede
GitHub-kontrol finder dublerede statusser, open/done-modstrid, manglende
blokårsag og for mange aktive issues. WIP-kontrollen er en preflight, ikke en
atomisk lås mellem konkurrerende GitHub-skrivere. Derfor er hovedagenten den
eneste koordinator for statusovergange. Kontrollerne beviser ikke i sig selv
rigtigheden af bogføring eller et citeret testresultat.

## Brug

```sh
python3 scripts/workflow/rewrite.py status
python3 scripts/workflow/rewrite.py check-remote --include-epic
python3 scripts/workflow/rewrite.py packet --issue 701 --owner /root \
  --path exact/file.rs --path owned/directory/ \
  --acceptance 'Det konkrete brugerforløb fungerer' \
  --check 'Den aftalte målrettede testkommando'
python3 scripts/workflow/rewrite.py install-hooks
```

Erstat eksempelstierne med opgavens faktiske afgrænsning; brug ikke hele
repository-roden som filansvar. Før et commit kontrollerer hooks både staged
stier og en enkelt afsluttende linje i commitbeskeden:

```text
Work-Issue: #701
```

Hovedagenten fører et færdigudviklet issue videre med begrundelse:

```sh
python3 scripts/workflow/rewrite.py move 701 integrating --reason 'De aftalte dele er afleveret'
python3 scripts/workflow/rewrite.py move 701 acceptance \
  --reason 'Integreret og målrettet verificeret' --evidence 'reference-til-resultat'
```

Luk med `move 701 done --reason 'Hele issuet er accepteret' --evidence /absolut/closeout.json`.
Lukningskvitteringen er en flad JSON-fil, ikke en ny kø:

```json
{
  "issue": 701,
  "issue_body_sha256": "SHA256 af den aktuelle fulde GitHub issue-body som UTF-8",
  "revision": "40 tegn: accepteret source-commit",
  "whole_issue_accepted": true,
  "remaining": [],
  "review": "Hovedagentens konkrete vurdering af diff, fuldt scope og bevis",
  "criteria": [
    {"criterion": "Hele teksten fra første oprindelige acceptance-checkbox", "passed": true, "evidence": "rapportreference med konkret resultat"}
  ]
}
```

`criteria` skal indeholde **alle** checkboxtekster under `## Acceptance criteria`
(eller epic'ens `## Completion criteria`) i samme rækkefølge. Bevar oprindeligt
scope og nødvendige scenarier; forkort eller redigér ikke krav for at få lukket.
Scriptet afviser delaccept, manglende kriterier, en ændret issue-body og aktiv
blokering. GitHubs compare-API skal bekræfte, at source-committet er publiceret i
`codex/rust-platform`, inden der skrives nogen kommentar eller lukkes.
Kvitteringen kopieres til GitHub-kommentaren; et lokalt filnavn alene er ikke
leverancebevis. PATCH-svaret skal bekræfte issueidentitet og den ønskede status,
ellers skal faktisk tilstand afstemmes før et nyt forsøg.

Kontrollen validerer struktur, scopebinding, publicering og GitHub-svar. Den kan
ikke bedømme, om en evidensreference eller reviewkonklusion er sand; hovedagenten
skal stadig læse den faktiske diff og de konkrete resultater. De eksisterende
hooks håndhæver filansvar og issuebinding, ikke semantisk fravær af overengineering.
Historiske lukkede issues genåbnes ikke af denne fremadrettede kontrol.
En konkret acceptfejl bruger `move 701 developing --reason 'Den påviste fejl'`.
Installér hooks én gang pr. clone; Git deler dem med linked worktrees.
SGD leverer arbejdspakken automatisk; manuelle pakker oprettes separat i hvert worktree. Gamle branches uden
workflow-scriptet er undtaget, så installationen ikke ødelægger andet arbejde.

Lokale workflowtests:

```sh
python3 -m unittest discover -s scripts/workflow -p 'test_*.py' -v
```


## Fuld oversigt og automatisk fortsættelse

Det eksisterende SGD-board på `http://127.0.0.1:8743` viser både alle55
oprindelige issues og den aktuelle udførelse. Issueoversigten er en read-only
GitHub-projektion med status, afhængigheder, åbne acceptkriterier og bevaret
delaccept. Åbne issues er ikke et mål for andelen af kode, der mangler. Ingen
lokal kopi af oversigten må overtage leverancestatus eller blive en ekstra kø.

Opdatér fra hovedcheckoutet med:

```sh
python3 scripts/workflow/board_snapshot.py --project /Users/mikkelfreltoftkrogsholm/.codex/worktrees/rust-sgd-pilot/rentemester
```

Codex-heartbeat `rentemester-rust-automatisk-fremdrift` er sat på **pause efter
brugerens stop**. Denne ændring af styringen genoptager hverken automatik eller
Rust-udvikling. Ved en senere autoriseret genoptagelse bruges samme heartbeat. Hovedagenten afstemmer processer/resultater, opsamler afsluttede
workers, reviewer konkrete diffs og checks, retter diagnosticerede fejl og
integrerer gennem SGD. En bestået test er aldrig automatisk reviewgodkendelse.
Efter hel issueaccept publiceres den accepterede kode, beviset registreres, og
issuet lukkes faktisk i GitHub. **Først derefter** vælges næste nødvendige issue.
En accepteret underleverance fører til samme issues restaccept; den tæller ikke
som et lukket issue. Kun dokumenteret blokering eller brugerstop tillader parkering. Eksisterende
WIP-, worktree-, plads-, regnskabs-, adgangs- og acceptkontroller bevares.

`.sgd/driver.json` viser kun styringens konfiguration og seneste observerede
handling. Den ejer ingen opgaver. Heartbeat kører via Codex' understøttede
planlægning, og lokal eksekvering kræver en tilgængelig vært. Boardets to-sekunders
visningsopdatering starter ingen arbejde. Opfølgningen er stille ved uændret
status og stoppes ved brugerens pause/stop eller hele målets verificerede
færdiggørelse. Installeret manuel desktopaccept ligger stadig allersidst.
