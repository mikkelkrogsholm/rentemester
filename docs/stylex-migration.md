# StyleX i hele Rentemester

Denne ændring samler produktets ejede visuelle styling i StyleX. Cockpit,
offentlig hjemmeside, HTML-eksporter, PDF-skabeloner og genererede SVG/OG-billeder
bruger samme authoring-teknologi. DESIGN.md er autoritet for både produktets
papirpalette og hjemmesidens bevarede mørke marketingpalette.

## Build og udvikling

Cockpittet bevarer sin Bun/StyleX-compiler, lokale fontressourcer og hashed CSS.
Alle controls, responsive recordmønstre, fokus, print og root/body-styles er
direkte StyleX-kompositioner. Komponentvarianter er eksplicitte; gamle
klassestrenge bruges ikke til at vælge en variant.

`scripts/stylex-precompile.ts` kompilerer selvstændige StyleX-kilder til fuldt
sammensatte attributkort og deterministisk CSS. Dokumenternes genererede
TypeScript er en del af runtime; runtime skal ikke have Babel eller en browser
for at producere HTML. Kontrollér drift med:

```sh
bun run design:check
bun scripts/document-styles.ts --check
bun run stylex:check
```

Hjemmesiden har fortsat eget Astro-build, package-manifest og lockfile. Astro
afventer prekompilering ved start og linker selv til det genererede stylesheet.
Dev-watcher publicerer stylesheetet før attributmodulet og genindlæser siden.
Ved compilefejl bevares sidste konsistente generation; et produktionsbuild
fejler. Compileroutput i www/.stylex og www/public/_stylex er buildcache.

## Navigation og SVG-grafer

Portefølje og Opgaver er synlige workspaceindgange. CFO-overblik følger de
eksisterende deployment- og adgangsregler. Virksomhedens mobile navigation hedder
Selskabsmenu og bruger samme autoriserede rutekatalog som desktop.

Opgaver skelner visuelt mellem scope, visning, aktive filtre og handlinger.
Opret opgave er primær; kildeopdatering er sekundær. Visningsvalg og hurtigfiltre
har aria-pressed og synlig valgt tilstand. Listekonteksten bevares i URL'en.

Før gennemgangen lå Portefølje og Opgaver i Workspace-menuen, mens Opgavers
visningsvalg og hurtigfiltre fremstod som to rækker ens handlinger. Nu er
Portefølje og Opgaver direkte destinationer med synlig aktiv markering.
Opgavevisningen viser selskabsscope over ét samlet visningsvalg, fulgt af
hurtigfiltre og et separat filterpanel. Aktive selskaber og filtre forbliver
synlige, når panelet er lukket. Den primære oprettelse og den sekundære
kildeopdatering er bevaret på både desktop og mobil.

De fem økonomigrafer bruger fælles SVG-rendering. Geometri er beregnede data;
farver, streger, tekst og layout defineres i StyleX. Serieknapper bevarer skjul/vis,
og periodeværdier kan inspiceres med tastatur, hover og touch. Regnskabstabeller,
valuta, delvise år, negative værdier og ukendt banksaldo bevares.

## PDF og historiske beviser

PDF bruger lokale, tilpassede pdfcn-komponenter og takumi-pdf 0.15.0. Komponenterne
producerer semantisk HTML med prekompilerede StyleX-styles. Skrifter og licenser
ligger i src/design/fonts; rendering kræver ingen netværksressourcer.

Selvstændige HTML-rapporter registrerer deres indlejrede skrifter med FontFace.
iXBRL er fortsat scriptfri XHTML og bruger dokumentets font-fallback, hvis den
valgte skrift ikke findes på læserens maskine. Brede rapporttabeller har en
navngivet, fokusérbar scrollregion; print fjerner denne begrænsning.

En afgrænset synkron Bun-renderproces bevarer regnskabsfunktionernes buffer-
og transaktionskontrakter. Den modtager dokumentinput gennem stdin og returnerer
PDF-bytes på stdout. Timeout eller rendererfejl udløser den eksisterende rollback
af nummerreservation, dokumentrækker og filpublicering.

Udstedte faktura-PDF'er og deres hashes omskrives ikke. Afsendelse læser det
udstedte, hashverificerede PDF-bevis. Eksisterende leveringskvitteringer bevarer
deres message-id og idempotens. Nye HTML/PDF-eksporter kan få nye markup- og
bytehashes som følge af den nye rendering.

PDF-sideformat og SVG-path-data er dokumentgeometri. Takumis pageNumber og
totalPages er rendererens tællermarkører, har ingen CSS-regler og er kun tilladt
i den konkrete pdfcn-adapter. Kompilerede stylesheets og StyleX-genererede
attributter er output, ikke en alternativ stylingkilde.

## Afleveringskontrol

Den samlede aflevering kræver følgende evidens:

| Område | Kontrol |
| --- | --- |
| Styling | Ejerskabskontrol uden rester af håndskrevet CSS/Tailwind; token- og dokumentartefakter reproducerbare |
| Cockpit | Komponent- og browsertests; mobil, tastatur, dialog, print og forsinket lokal fontindlæsning |
| Grafer | Serievalg, keyboard/touch, nul/negative/null, valuta, to akser og tomme datasæt |
| Hjemmeside | Selvstændig ren installation/build; Chromium/WebKit; menu, artikler, kode, beregner og 404 |
| Dokumenter | Offline HTML, XML-safe iXBRL, syntetiske snapshots og uændrede regnskabsdata |
| PDF | Danske tegn, lange tabeller, sideskift, tekstudtræk, determinisme og timeout/rollback |
| Historik | Gemte PDF-hashes samt gentagen afsendelse med gamle leveringskvitteringer |
| Distribution | Fuld lokal verifikation, container-smoke, reproducerbar OCI og verificeret releasekandidat |

Integreret lokal evidens fra 8. oktober 2026:

| Gate | Resultat |
| --- | --- |
| Frossen installation, discovery, runtime-/app-/E2E-typecheck, lint, design- og StyleX-kontrol | Bestået |
| Backend | 3.054 tests, 25.557 assertions, ingen fejl |
| Cockpitkomponenter | 663 tests, 2.696 assertions, ingen fejl |
| Cockpitbrowser | 336 forløb i Chromium og WebKit, alle ruter, desktop/320 px, login, fokus, SVG/touch, print og reduced motion |
| Selvstændigt website | 4 compilertests/34 assertions, 81 byggede sider og 28 browserforløb; SVG-driftkontrol bestået |
| Dokumentbrowser | 16 Chromium/WebKit-forløb på desktop/mobil med egne offline skrifter, ægte XHTML-MIME for iXBRL, tastaturscroll og print |
| Forsyningskæde | Ingen advisories; 131 produktionspakker med tilladte licenser |
| CLI/MCP | Komplet syntetisk smoke og fakturalivscyklus bestået |
| Container | Takumi-PDF med danske/Latin Extended-tegn, lange faktura-/rapporttabeller, selekterbar tekst, deterministiske bytes og timeout; constrained/networkless/non-root/read-only runtime og genstart bestået |

`verify:local` blev genoptaget ved den fejlede gate efter rettelse af en gammel
versions-/navigationsassertion; de beståede backendtrin blev bevaret. Det
udvidede browserreview fandt og rettede den smalle MFA-overskrift, kladde-filterets
intrinsiske bredde og overlappende responsive regler. Den komplette integrerede
browserkørsel ovenfor er efter disse rettelser. Kontrollen kræver ingen ændringer
i virkelige ledgers; alle mutationsforløb bruger syntetiske workspaces.

OCI-reproducerbarhed og GitHub-kandidatens præcise identitet følger releaseflowet
i [releasevejledningen](release/README.md). Kandidaten er ingen produktionspromotion.
