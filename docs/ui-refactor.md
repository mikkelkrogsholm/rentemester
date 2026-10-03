# UI-refaktorering: kontrakter, dækning og verifikation

Cockpit-appen bruger projektets lyse papir/blæk-design fra `DESIGN.md`. Marketingwebsitets særskilte design er et selvstændigt produktområde. Regnskabsregler, beløb, ledgerkontroller, planhash og adgangsafgørelser er fortsat serverens ansvar.

## Dækningsmatrix

Udgangspunktet var 48 view-filer og 37 virksomhedsindgange. Nogle filer indeholder flere tilstande, mens onboarding indlejres i porteføljen. Nye detalje- og oprettelsesruter er arbejdsflows under eksisterende destinationer og skal ikke duplikeres i navigationen.

`R` betyder, at `routes.spec.ts` har en browsertest med en eksplicit, korrekt formet succesrespons og visningsspecifikt indhold. `M` betyder, at samme side har en 320 px-test for indhold, JavaScript-fejl og sidebredde samt et skærmbillede. `F` betyder en særskilt adfærdstest i `core-workflows.spec.ts` eller `auth-and-state.spec.ts`. Markeringerne beskriver den integrerede testdækning. En læsesmoke dokumenterer ikke i sig selv, at samtlige mutationer er browserverificeret.

Permissions i tabellen er forkortet: `read` = `company.read`, `documents.read/upload`, `master-data`, `draft.write`, `ledger.post`, `review`, `external-send`, `admin`, `knowledge.*`, `ownership.*` og `export` har alle præfikset `company.`. Workspace-permissions angives fuldt. Den fælles policy er autoritativ; hver handling skal bruge sin eksisterende serverpermission, også når en side indeholder flere slags handlinger.

| Destination | Handlinger og kontrolpunkter | Rettigheder | Scope | Browserdækning |
|---|---|---|---|---|
| Overblik | Opgaver, frister, diagrammer, drill-down, løsning af undtagelse | read; review for beslutninger | Regnskabsår | R, M |
| Posteringer | Søgning, kontofilter, bilag, CSV, dimensionsgennemgang og erstatning | read, export; ledger.post for bogføringsændringer | Regnskabsår | R, M |
| Kladder | Opret, revidér, indsend, afvis, uafhængig godkendelse og bogføring | draft.write, review, ledger.post efter handling | Virksomhed | R, M |
| Posteringsregler | Læs, foreslå, forklar, godkend, deaktivér og supersedér | read; eksisterende policy for hver regelhandling | Virksomhed | R, M |
| Bogføringskø | Filter, plan, gem eksakt plan, godkend, anvend, varig historik og kvittering | read, draft.write, review, ledger.post efter trin | Eksplicit datointerval | R, M; workflow- og populationskontrol i komponenttests |
| Bank | Søgning, import, afstemning, korrektion og bilag | read; master-data/ledger.post efter handling | Regnskabsår | R, M, F for skift og sene svar |
| Bilag | Filter, indlæsning, filer, posteringer, partsreview og bogføring | documents.read/upload; review og ledger.post efter handling | Alle virksomhedens bilag; årsvalg er bogføringskontekst | R, M, F for roller og bogføring |
| Leverandørfaktura | Registrér, betal, kreditér/ret efter eksisterende workflow, evidens | read; ledger.post efter handling | Virksomhed og forfaldsfilter | R, M |
| Kørsel | Registrér og gennemgå ture og kilometer | read; ledger.post efter handling | Regnskabsår | R, M |
| Anlæg | Registrér, næste afskrivning, afskriv og afskriv ved afgang | read; ledger.post efter handling | Virksomhed | R, M |
| Agent-forslag | Gennemgå evidens, godkend, afvis og åbn relateret arbejde | read, review | Virksomhed | R, M |
| Undtagelser | Åbne/løste, evidens og løsning med begrundelse | read, review | Virksomhed | R, M |
| Fakturaer | Søgning, udstedelse, betaling, kredit, PDF, mail, PEPPOL-status og rykkere | read, draft.write/ledger.post; external-send for levering | Regnskabsår | R, M, F for oprettelse og fejl |
| Faktura-skabeloner | Opret, generér, træk tilbage og se historik | read; draft.write/ledger.post efter handling | Virksomhed | R, M |
| Kontakter | Opret, redigér, slet, CVR-opslag og relationer | read, master-data; external-lookup for opslag | Virksomhed | R, M, F for native bekræftelse og afvisning |
| Moms | Periodetal, TastSelv-tal, PDF, readiness, review, luk og genåbn | read, export; ledger.post/admin/period.force-close efter handling | Regnskabsår og momsperiode | R, M |
| Periodelås | Readiness, review, luk, genåbn og begrundet force-close | read; ledger.post/admin/period.force-close efter handling | Eksplicit periode | R, M |
| Periodisering | Register, genkendelse og kontohenvisninger | read | Virksomhed | R, M |
| Resultatopgørelse | Kontodrill-down, sammenligning, CSV og PDF | read, export | Regnskabsår | R, M |
| Balance | Kontodrill-down, sammenligning, CSV og PDF | read, export | Regnskabsår | R, M |
| Saldobalance | Kontodrill-down, debet/kredit, CSV og PDF | read, export | Regnskabsår | R, M |
| Forpligtelser | Frister, beløb og relaterede konti | read | Regnskabsår | R, M |
| Likviditet | Pengestrøm, saldo, commitments, match, pause og afslutning | read; eksisterende policy for commitmenthandlinger | Regnskabsår og forecastdato | R, M |
| Budget | Plan, beløb, sammenligning og dimensionsfordeling | read; admin for budgetændringer | Regnskabsår | R, M, F for læserens skrivebeskyttelse |
| Flerår | Sammenlign år, arkiv/live og grafer | read | Flere regnskabsår | R, M |
| Årsrapport | Vælg periode, serverens forudsætninger, regnskab og noter | read, export | Eksplicit regnskabsperiode | R, M for formularen; rapportresultater i komponenttests |
| Workspace-register | Parter, immutable records, assertions, ejerforhold, review og eksakt apply | knowledge.read/manage, ownership.read/manage efter handling | Virksomhed og pr. dato | R, M |
| Fælles indbakke | Indlæs kilde, review, eksplicit virksomhedstildeling og separat handoff | documents.read/upload; eksisterende workspacepolicy | Workspace via autoriseret virksomhed | R, M |
| Arkiv | Arkivforklaring og årsindgange | read | Virksomhed | R, M |
| Virksomhedsoplysninger | Navn, profil, CVR-sync, import, eksport og arkivering | admin; external-lookup/export efter handling | Virksomhed | R, M |
| Retention | Opbevaringsstatus, udløb og lovhenvisninger | read | Virksomhed, pr. dato | R, M |
| Integritet | Kæde, backup, destinationer og verificér igen | read | Virksomhed, pr. tidspunkt | R, M |
| Kontoplan | Søgning, typefilter og fail-closed kontoroller | read | Virksomhed | R, M |
| Dimensioner | Opret, omdøb, aktivér/deaktivér definitioner og medlemmer med historik | read; master-data efter handling | Virksomhed | R, M |
| Bankkonti | Registrér konto og gennemgå importprofiler | read; master-data efter handling | Virksomhed | R, M |
| GDPR | Søgning, indsigt, retentionkontrol og anonymisering af det gennemgåede subjekt | export for indsigt; admin for anonymisering | Virksomhed og gennemgået subjekt | R, M for formularen; subjektbinding og resultater i komponenttests |
| Bilagsmail | Alias, IMAP-konfiguration, slet konfiguration og indbakke | admin efter eksisterende policy | Virksomhed | R, M |

### Workspace, adgang og indlejrede tilstande

| Visning/tilstand | Handlinger | Rettigheder/gate | Scope | Browserdækning |
|---|---|---|---|---|
| Portefølje | Opgaver, nøgletal, virksomhedsskift og tom workspace | workspace.read | Workspace | R, F for deploymentprofiler og axe |
| Onboarding | Første virksomhed, stamdata og bankoplysninger | workspace.manage | Workspace | Indlejret i portefølje; eksisterende komponenttests |
| Tilføj virksomhed | Stamdata, momsperiode, CVR og oprettelse | workspace.manage | Workspace | R for formularen |
| CFO-overblik | Virksomhed/portefølje/koncern, periode, evidens og kildehenvisninger | workspace.read/group.read og firmaadgang | Eksplicit scope og periode | R; beregnings-/koncernvarianter i komponenttests |
| Koncernstruktur | Struktur, delvis synlighed, afstemning, profil og dispositionsstatus | workspace.group.read | Pr. dato | R; fail-closed varianter i komponenttests |
| Brugere | Invitation, annullering, medlemskab og firmaadgang | workspace.members.read/manage | Workspace | R for læsning; mutationer i komponenttests |
| Hjælp | Produktvejledning og supportoplysninger | AuthGate i hosted | Global | R |
| Lovgrundlag | Søgning, bundlefilter, regler og kilder | AuthGate i hosted | Global | R |
| Login | Adgangskode, TOTP og recovery code | Hosted uden session | Bruger | F for loginpanel; credential-/TOTP-resultater i komponenttests |
| Glemt adgangskode | Reset-anmodning med ensartet offentlig kvittering | Hosted uden session | Bruger | F for ukendt konto |
| Ny adgangskode | Tokenrensning, minimumslængde og reset | Hosted uden session | Bruger | F for panel; resultat i komponenttests |
| E-mailverificering | Recovery og krævet verificering før adgang | Hosted uden verificering | Bruger | F; firmaendpoint må ikke kaldes |
| MFA-opsætning | Start, TOTP, recovery codes og bekræftelse | Hosted verificeret uden MFA | Bruger | F for adgangsgate; opsætning i komponenttests |
| Invitation | Token, ny bruger og næste adgangstrin | Public invitation gate | Bruger/workspace | F for panel; claim i komponenttests |
| Kontomenu | Sessioner, adgangskode og logout | Hosted session | Bruger | F for native dialog, mobilbredde, fokusretur og rydning af passwordfelter; mutationer i komponenttests |
| Ukendt URL | Forklaring og retur til portefølje | Normal adgangsgate | Global | R |
| Ny faktura | Lang formular, danske beløb, preview, udstedelse og usaved-guard | Company permission og arkivgate | Valgt regnskabsår | F |
| Fakturadetalje | Identitet ved documentId, status, PDF og eksisterende handlinger | Company permission pr. handling | Regnskabsår | F for deep link, filterretur og afbrudt afstemning |
| Bilagsdetalje | Dokument, fil, samtlige tilknyttede posteringer og næste handling | documents.read | Alle virksomhedens bilag | F for deep link, alle posteringer og filterretur |
| Bogfør bilag | Bilag, bank, konto, momsvalidering og serverens resultat | ledger.post og periodegate | Bilag + valgt regnskabsår | F for valg, afvisning, kvittering og læsende kontrol efter afbrudt bogføring |

## Implementerede fælles kontrakter

`DESIGN.md` er tokenkilden. Generatoren afviser ukendte referencer og cykler og skriver TypeScript til statiske rapporter/diagrammer, StyleX-variabler til React og en CSS-bro til semantiske layouts. `design:check` afviser drift. Ingen virksomhedsdata indgår i tokens eller produktdefaults.

De fælles komponenter ejer native controls, varianter, fokus, beløb, overskrifter, status, sideskift og kvitteringer. Domænesiderne beholder deres eksisterende serverkontrakter. StyleX kompileres med den native Bun-adapter; transformkørsler serialiseres, fordi adapterens samtidige CSS-skrivninger ellers kan overskrive den sidste komplette fil. Produktionsbuildet afviser manglende designvariabler og linker den kompilerede CSS med indholdshash. Browserchecks undersøger den faktiske CSS: 248 px sidebar, Source Serif 4, 32 px H1, 44 px controls og designfarver.

Skrifterne hentes fra lokale Fontsource-pakker; tre OFL-notices følger `app/dist/licenses`. Der er ingen font-CDN. Galleriet startes med `bun run --cwd app gallery` på port 5320 og indeholder syntetiske tilstande. Det indgår ikke i produktionsruterne.

Adgangspolicyen i `src/core/access-permissions.ts` deles mellem server og klient. Workspaceejerskab giver ikke implicit ejerskab af virksomheder. Læserens budgetfelter er read-only; handlinger såsom skabelontrækning, dimensionsreview, eksport og anonymisering bruger deres præcise permissions. GDPR-anonymisering bindes til det subjekt, hvis rapport er gennemgået; redigering af søgefelter ugyldiggør rapporten.

`useAsync` skjuler data fra en tidligere virksomhed/periode før næste effekt, afbryder gamle læsninger og bevarer strukturerede API-fejl. Samme ressources mislykkede statusopdatering bevarer formularen og den seneste læsning med en synlig advarsel. Reads bruger AbortSignal; mutationer genforsøges aldrig automatisk.

Uafklarede mutationer blokeres af `MutationMemoryProvider` og `useMutationOutcome`/`ConfirmDialog`. Opaque operation-ID'er gemmes i sessionStorage uden felter, modtagere eller GDPR-subjekter. Blokeringen overlever route-/filterskift, genåbning og reload i samme fane. GET-kontrol frigiver aldrig en handling. Fakturaoprettelse viser serverens seneste fakturaer for det valgte fiskalår; bilagsbogføring viser dokumentets registrerede journalposter. Brugeren kan først frigive efter en separat, eksplicit bekræftelse af, at resultatet er kontrolleret; frigivelsen sender ingen mutation.

Hvis browserens sessionslager afvises, blokeres skriveruter i klienten allerede før første handling, og UI viser lagerfejlen. Blokeringen kan ikke frigives, mens lageret fejler. Hvis lagring fejler efter en afbrudt handling, skal den åbne fane bevares, indtil blokeringen kan gemmes og resultatet er kontrolleret. Dette er en klientbeskyttelse i den aktuelle fane, ikke serveridempotens på tværs af faner, enheder eller manuelt ryddet browserlager. Eksisterende serveridempotency-, planhash-, revision- og populationskontroller er bevaret.

Ugemte felter beskyttes ved navigation, kontekstskift, før browserlukning og ved modalens Cancel/Escape/backdrop. Succes markerer formularen gemt før retur til listen. Native dialoger holder fokus, gør baggrunden inert og returnerer fokus; lange arbejdsflows bruger almindelig sidehøjde med lokal vandret rapportscrolling.

## Browserharness og kommandoer

Browserne kører mod det kompilerede `app/dist` på en særskilt server bundet til `127.0.0.1`. Serveren genbruger ingen eksisterende listener, sender ingen forespørgsler til Docker eller en virkelig workspace og afviser alle `/api/*`-kald uden browserinterception. Manglende fixtures fejler tests. Mutationer kræver deres egen eksplicitte fixture.

`app/e2e/data/core.json` genereres deterministisk fra de typed, syntetiske Bun-fixtures. JSON-broen er nødvendig, fordi Playwright kører i Node og Bun-fixtures importerer `bun:test`. Efter en ændring i de fælles fixtures regenereres filen; `--check` afviser drift.

```sh
bun app/scripts/browser-fixtures.ts --check
bun run cockpit:build
bunx --no-install playwright test --config app/playwright.config.ts
```

Browserbinaries installeres med `bunx --no-install playwright install chromium webkit`, hvis de mangler. `RENTEMESTER_BROWSER_TEST_PORT` vælger en anden fri port. Chromium kører hele matricen; WebKit kører de centrale flows, native dialoger, mobilnavigation, beløbsinput og tilgængelighedskontrol. Tests genforsøges ikke automatisk.

Tracing, fejlskærmbilleder og syntetiske layoutskærmbilleder skrives under `app/e2e/artifacts/`, som er ignoreret. De er gennemgangsartefakter; de er ikke automatisk en godkendt visuel reference. Ingen virkelige virksomhedsdata må anvendes til de versionsstyrede fixtures eller skærmbilleder.

## Accept og resterende menneskelig kontrol

Automatiske checks omfatter alle routeindgange, sidebredde ved 320 px, centrale layouts ved 320/390/768/1024/1440 px, faktiske native dialoger, tastatur/fokus/inert, ændrede formularer, dataskift med sene svar, rolleniveauer, hosted gates og sessionudløb. Axe undersøger de centrale sider mod WCAG A/AA-tags uden at skjule regelbrud. Fonts og øvrige appressourcer må ikke kræve eksterne netværkskald.

Afslutning kræver også grønne isolerede komponenttests, typecheck, lint, token-/fixturekontrol, produktionsbuild og relevante integrationstests. Bevar assertions for regnskab, routes og auth, når forældede CSS-klassechecks erstattes.

Manuel skærmlæsergennemgang af navigation, fakturaoprettelse og bilagsbogføring samt manuel vurdering ved 200 % zoom skal registreres med browser, platform, tidspunkt og observationer. Automatiske axe-resultater er ikke en attest for fuld WCAG-overholdelse. Fuld browsermutationstest af hver administrationsfunktion og godkendte visuelle snapshots er særskilte acceptpunkter og må ikke markeres færdige alene på baggrund af læsesmoken.

## Verificeret 3. oktober 2026

| Kontrol | Resultat |
|---|---|
| `bun run cockpit:test` | 570 tests, 0 fejl; 67 filer og 1.500 assertions |
| `bun run cockpit:test:browser` | 167 tests, 0 fejl i Chromium/WebKit; token-/fixturekontrol, E2E-typecheck og produktionsbuild indgår |
| `bun run typecheck:runtime` | Bestået |
| `bun run lint` | Bestået; 1.165 filer |
| Design-, dashboard-, Bun-cutover-, releaseworkflow- og licenskontrakttests | 83 tests, 0 fejl; 500 assertions |
| `bun run supply-chain:licenses` | Bestået; 132 produktionspakker, herunder OFL-fontlicenser |
| Galleriet mod den faktiske Bun-devserver | Source Serif 4/32 px H1, 44 px controls og native dialog/fokus bestået i begge browsere; ingen JavaScript-fejl |
| Visuel gennemgang | Syntetiske desktop-/mobilskærmbilleder af overblik, bilag, fakturaoprettelse og bogføringskø gennemgået; ingen godkendt snapshotbaseline erklæret |
| `graphify update .` | AST-opdatering gennemført; 9.206 noder og 32.524 kanter. Parseren rapporterede delvis ekstraktion i 67 filer, bl.a. marketingwebsitets Astro-filer |
| `bun run test:parallel` | 2.661 bestået, 0 fejl; 415 filer og 18.874 assertions |
| `bun run supply-chain:audit` | 0 advisories; 259 kontrollerede pakker |
| `bun run verify:local` | Bestået fra ende til anden: frozen install, discovery, typecheck, lint, audit/licenser, PDF-benchmark, backend-/app-/browsertests, CLI-/MCP-smoke, containerdrift og reproducerbare OCI-eksporter |

De fem backendfejl blev først reproduceret på det uændrede udgangspunkt i en isoleret checkout og er efterfølgende rettet under kandidatforberedelsen. De angik discovery/MCP-kataloget: registrycoverage, service-principal-mappets fuldstændighed og tre dokumentationskontroller af toolantal. Discovery-baselines er nu gennemgået for `invoice_imported_receivables` og CLI-pendanten. Importworkflowet beskriver den sikre læsning og grænsen til native fakturahandlinger. Rettighedstesten sammenligner alle toolnavne med den faktiske registrering og kontrollerer tilladt virksomhed, afvist virksomhed og tilbagekaldte credentials. Dokumentationen stemmer med 214 tools: 91 reads, 122 almindelige writes og én destructive operation; delsummerne kontrolleres også fra live annotations.

De oprindelige 16 advisories i MCP-afhængighederne er fjernet med kompatible, låste opdateringer til [fast-uri 3.1.8](https://github.com/advisories/GHSA-hrr3-gc8f-f4qj), [hono 4.13.7](https://github.com/advisories/GHSA-hxh3-vqpv-xpqv), [ip-address 10.7.1](https://github.com/advisories/GHSA-h3mg-xc3c-68pw) og [qs 6.16.0](https://github.com/advisories/GHSA-4mjr-xmp4-gh2g). Direkte dependencyranges er bevaret. Audit og lockfile-bundet supply-chain-evidens rapporterer nul advisories, og licenskontrollen består for 132 produktionspakker.

Den fulde gate fandt også en manglende Docker-buildafhængighed: cockpitstadiet kopierer nu `DESIGN.md`, token-scriptet, de genererede roottokens og den fælles browseregnede rettighedspolitik før build. Containerkontrollen består med begrænset, netværksløst runtime, read-only filesystem, PDF-tekst/layout, cache og persistens efter genstart. To separate builds uden cache producerer samme OCI-manifest og byteidentiske arkiver. Dette er lokal verifikation; kandidatworkflowet binder senere et publiceret image til sit konkrete commit og releaseidentitet.

To ustabile testopsætninger er repareret uden at svække assertions. CLI-testen får en ledig loopbackport fra operativsystemet; den tidligere tilfældige port kunne ramme port 5000, som bruges af en lokal systemtjeneste. Fejlen blev reproduceret med det tvungne gamle portvalg og bestod efter rettelsen. MCP-testens eventtæller læser nu et snapshot, så testens egen observation ikke åbner en skrivbar forbindelse og påvirker WAL/checkpoint eller den mtime, som kontrolleres. Kravet om uændret ledger-mtime og korrekt append-only/idempotent apply er bevaret.

Manuel skærmlæserkontrol og faktisk 200 % browserzoom er endnu ikke udført. Mobilreflow, tastatur, inert/fokus og axe-checks er automatiseret; resultaterne dækker de anførte tests og syntetiske tilstande, ikke en generel attest for alle kombinationer eller fuld WCAG-overholdelse. Der er ikke deployet eller skrevet til virkelige virksomhedsdata.

## Ekstra designreview 3. oktober 2026

Den fælles designkontrakt er gennemgået med et uafhængigt komponent-/tilgængelighedsreview og efterfølgende integreret kontrol. `DESIGN.md` dokumenterer nu alle 52 offentlige React-komponenter fra 29 fælles kildefiler, deres familier og relevante helpers. Kataloget omfatter primitives, shell/navigation, kontomenu, alle domænedialoger/formularer, feedback, grafer, virksomhedskort/-formular og revisor-eksport. Det præciserer brug, varianter, datakontekst, responsive mønstre, tastatur, fokus, permissions og asynkrone tilstande.

| Fund | Rettelse og evidens |
|---|---|
| Manglende fælles komponenter og adfærdsregler i designfilen | Komplet katalog med ejerskab, modal-/sidevalg, formularregler, status/feedback, tabeller, grafer og vedligeholdelse. En kontrakttest finder udokumenterede nye exports fra komponentmappen/UI-entrypoint. |
| Aktive feltkanter havde ca. 1,93:1 kontrast mod papir | `border-strong` er nu varm grå `#8F887D`: ca. 3,11:1 mod paper og 3,31:1 mod paper-raised. Tokens er regenereret. Kontrasttest kontrollerer aktive kanter og fokus; browsertest kontrollerer den kompilerede feltkant. |
| Modalens Tab-forløb sprang native summary over og medtog skjulte controls | Fælles fokusforløb inkluderer native disclosures og udelukker lukkede/hidden/inert/disabled felter. Mobiltesten åbner en lukket gruppe med tastaturet, går frem/tilbage og navigerer med bevaret år i begge browsere. |
| Destruktiv bekræftelse startede på udfør-handlingen | Fokus starter på Annullér. Native browserkontrol verificerer startfokus, inert baggrund, Tab-forløb, Escape og fokusretur. |
| Kontopaneler manglede modalitet, udløserrelation og ensartet lukning | Sessioner og passwordformular bruger Dialog med aria-haspopup/expanded/controls og returnFocusRef til asynkron åbning. Passwordfelter ryddes ved dismiss, succes og fejl. 320 px-test kontrollerer fokus, reflow, Escape, genåbning og axe. Eksisterende unit-tests bevarer assertions for revocation og passwordændring. |
| Mobilmenuens Luk-knap lå efter en lang liste, og lange dialogtitler kunne overstige bredden | Luk-handlingen ligger øverst og forbliver synlig under scrolling; titler ombrydes. 320 px-menuens interne bredde, fokus og Luk-knappens placering kontrolleres i begge browsere. |
| Grafer var canvas uden et fuldstændigt tilgængeligt datagrundlag | Alle fire canvases har navn og eksplicit rapportvaluta. PnlChart har native fold-ud-tabel. Likviditetstabellen omfatter banksaldo, og integrerede grafer refererer med aria-details til strukturerede tabeller. Tests skelner mellem null, nul og negative saldi, bevarer EUR og mærker delvise år. Legend-/aksefont følger tokens; grafanimation er slået fra. Canvas gentegnes efter lokal fontindlæsning; en browsertest tilbageholder fontfilerne og verificerer tegning med den indlæste mono-font. |
| Loading havde ingen live-region; budgetvælger brugte et ufuldstændigt tab-mønster | Loading bruger polite status. Budgetvælgeren bruger native knapper i en navngivet gruppe med aria-pressed. Felt- og navigationsdisclosures har udløserrelationer; dekorative transitions respekterer reduced motion. |
| Eksportteksten beskrev databevægelse upræcist | Teksten angiver servergenerering og brugerens egen deling; Generér og download følger den præcise company.export-permission. |

Efter rettelserne er 570 app-tests og 167 Chromium/WebKit-tests bestået. De 83 relevante root-tests dækker også statiske dashboardudskrifter og designkontrakten. App-/E2E-typecheck, token-/fixturekontrol og produktionsbuild indgår i browsergaten; runtime-typecheck, lint og diff-kontrol er bestået særskilt.

Syntetiske skærmbilleder af passworddialog ved 320 px, graf med åben tabel på desktop og mobilnavigation er visuelt gennemgået. Diagrammet bevarer rolige papirflader og læsbare beløb; formularens labels, fokus og ombrudte handlinger er synlige. Disse billeder er gennemgangsevidens og ingen godkendt snapshotbaseline.

Manuel skærmlæserkontrol og faktisk 200 % browserzoom udestår fortsat. Bestået axe, keyboard og reflow er ikke en generel WCAG-attest. Backend-/dependencyresultaterne ovenfor er fra refaktoreringens oprindelige gate; de er ikke genkørt i dette afgrænsede designreview, som ikke ændrer regnskabsmotoren eller afhængighederne.
