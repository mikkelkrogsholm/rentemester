# Rust-omskrivningens arbejdsgang

GitHub-issues under [epic #672](https://github.com/mikkelkrogsholm/rentemester/issues/672)
er autoritative for opgavestatus. Bevar de eksisterende 55 issues, deres fulde
acceptkriterier og afhængigheder. Der oprettes ikke et nyt parallelt board.

```text
flow:backlog → flow:ready → flow:developing → flow:integrating → flow:acceptance → flow:done
                                    ↑                              │
                                    └──── konkret påvist fejl ──────┘
```

| Status | Betydning og overgang |
| --- | --- |
| backlog | Ikke aktuelt fokus. Tidligere implementering og evidens bevares; status betyder ikke, at intet er bygget. |
| ready | Næste afgrænsede resultat, filansvar, accept og nødvendige afhængigheder er afklaret. |
| developing | Hovedagentens aktuelle fælles brugerforløb; uafhængige dele kan udvikles parallelt. |
| integrating | Underleverancer samles af hovedagenten; ændringer kontrolleres sammen. |
| acceptance | Det integrerede forløb prøves mod acceptkriterierne. Kræver evidensreference. |
| done | Hele issuets accept er bevist, og issuet er lukket. Kræver evidensreference. |

Der skal være præcis én statuslabel pr. issue. `flow:blocked` er en ekstra
markering med en selvstændig linje `Flow-Blocked: konkret årsag` i issue-body;
den erstatter ikke status. Fjern markeringen og årsagen, når blokeringen er
løst. En genåbning eller retur fra accept kræver konkret begrundelse.

Et ufærdigt issue kan parkeres fra developing eller acceptance til backlog,
når næste nødvendige brugerforløb kræver et andet issue. Det kræver både en
konkret begrundelse med resterende arbejde og en evidensreference for den
bevarede leverance. Issuet forbliver åbent med sit fulde scope og tidligere
evidens. Genoptagelse går gennem ready og developing; parkering er ikke accept
af hele issuet og ændrer ikke grænsen på ét aktivt underissue.

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
begrundelse; acceptance/done kræver også en evidensreference. Statusændringen
bevarer øvrige labels og skriver et spor i issue-kommentarerne.

Git-hooks kræver en lokal arbejdspakke med issue, ejer, tilladte filstier,
accept og checks. `pre-commit` afviser staged ændringer uden for filansvaret;
`commit-msg` kræver `Work-Issue: #NUMMER`, der matcher pakken. Pakken ligger i
det enkelte worktrees Git-metadata og bliver ikke et ekstra opgaveboard.
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

Luk først med `move 701 done`, når **hele** issuets oprindelige accept er
opfyldt; også denne overgang kræver `--reason` og `--evidence`.
En konkret acceptfejl bruger `move 701 developing --reason 'Den påviste fejl'`.
Installér hooks én gang pr. clone; Git deler dem med linked worktrees.
Arbejdspakken oprettes separat i hvert worktree. Gamle branches uden
workflow-scriptet er undtaget, så installationen ikke ødelægger andet arbejde.

Lokale workflowtests:

```sh
python3 -m unittest discover -s scripts/workflow -p 'test_*.py' -v
```
