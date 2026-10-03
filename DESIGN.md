---
name: Rentemester
colors:
  paper: "#F4F1EB"
  paper-raised: "#FBF8F3"
  ink: "#1B1A17"
  ink-muted: "#4C4740"
  accent: "#A6332A"
  on-accent: "#F4F1EB"
  danger: "#8F2A22"
  success: "#2E5E4E"
  warning: "#8A5A12"
  info: "#2D5673"
  accent-soft: "#E8D7D3"
  danger-soft: "#EED9D6"
  success-soft: "#DCE8E1"
  warning-soft: "#EEE3D1"
  info-soft: "#D9E4EB"
  border: "#D8D2C6"
  border-strong: "#8F887D"
typography:
  headline-family: "Source Serif 4"
  body-family: "IBM Plex Sans"
  mono-family: "IBM Plex Mono"
  body-size: "16px"
  body-line-height: "1.5"
  mono-features: "tnum"
  size-xs: "12px"
  size-sm: "14px"
  size-md: "16px"
  size-lg: "18px"
  size-xl: "24px"
  size-2xl: "32px"
spacing:
  xxs: "4px"
  xs: "8px"
  sm: "12px"
  md: "16px"
  lg: "24px"
  xl: "32px"
  2xl: "48px"
layout:
  max-width: "1440px"
  sidebar-width: "248px"
  prose-width: "68ch"
  mobile-breakpoint: "640px"
  navigation-breakpoint: "1024px"
controls:
  height: "44px"
  compact-height: "36px"
  focus-width: "2px"
  focus-offset: "3px"
rounded:
  sm: "2px"
  md: "4px"
  lg: "8px"
components:
  button-primary:
    background-color: "{colors.ink}"
    text-color: "{colors.paper}"
    border-radius: "{rounded.md}"
    padding-inline: "{spacing.md}"
    padding-block: "{spacing.xs}"
  button-secondary:
    background-color: "{colors.paperRaised}"
    text-color: "{colors.ink}"
    border-color: "{colors.borderStrong}"
    border-radius: "{rounded.md}"
  button-danger:
    background-color: "{colors.danger}"
    text-color: "{colors.paper}"
    border-radius: "{rounded.md}"
  table-row:
    background-color: "{colors.paper}"
    text-color: "{colors.ink}"
    border-color: "{colors.border}"
  badge-status-paid:
    background-color: "{colors.successSoft}"
    text-color: "{colors.ink}"
    border-radius: "{rounded.sm}"
  badge-status-overdue:
    background-color: "{colors.dangerSoft}"
    text-color: "{colors.ink}"
    border-radius: "{rounded.sm}"
  alert-danger:
    background-color: "{colors.dangerSoft}"
    text-color: "{colors.ink}"
    border-color: "{colors.danger}"
  amount-cell:
    text-color: "{colors.ink}"
    font-family: "{typography.monoFamily}"
  button-quiet:
    background-color: "transparent"
    text-color: "{colors.ink}"
  form-control:
    background-color: "{colors.paperRaised}"
    text-color: "{colors.ink}"
    border-color: "{colors.borderStrong}"
    border-radius: "{rounded.md}"
    min-height: "{controls.height}"
    font-size: "{typography.bodySize}"
    padding-inline: "{spacing.sm}"
    padding-block: "{spacing.xs}"
  control-focus:
    outline-color: "{colors.info}"
    outline-width: "{controls.focusWidth}"
    outline-offset: "{controls.focusOffset}"
  field-help:
    text-color: "{colors.inkMuted}"
    font-size: "{typography.sizeSm}"
  field-error:
    text-color: "{colors.danger}"
    font-size: "{typography.sizeSm}"
  page-header:
    font-family: "{typography.headlineFamily}"
    font-size: "{typography.size2xl}"
    margin-bottom: "{spacing.lg}"
  dialog:
    background-color: "{colors.paperRaised}"
    text-color: "{colors.ink}"
    border-color: "{colors.borderStrong}"
    border-radius: "{rounded.md}"
    padding: "{spacing.lg}"
    max-width: "640px"
    viewport-inset: "{spacing.md}"
    backdrop-color: "rgb(27 26 23 / 40%)"
    title-size: "{typography.sizeXl}"
  navigation:
    sidebar-width: "{layout.sidebarWidth}"
    link-min-height: "{controls.height}"
  filter-bar:
    background-color: "{colors.paperRaised}"
    border-color: "{colors.border}"
    border-radius: "{rounded.md}"
    padding: "{spacing.md}"
  table:
    text-size: "{typography.sizeSm}"
    header-background: "{colors.paperRaised}"
    cell-padding: "{spacing.sm}"
    separator-color: "{colors.border}"
  badge-status-warning:
    background-color: "{colors.warningSoft}"
    text-color: "{colors.ink}"
    border-radius: "{rounded.sm}"
  badge-status-info:
    background-color: "{colors.infoSoft}"
    text-color: "{colors.ink}"
    border-radius: "{rounded.sm}"
  result-receipt:
    background-color: "{colors.paperRaised}"
    border-color: "{colors.border}"
    border-radius: "{rounded.md}"
    padding: "{spacing.md}"
  chart:
    income-color: "{colors.success}"
    expense-color: "{colors.accent}"
    comparison-color: "{colors.info}"
    legend-size: "{typography.sizeSm}"
    axis-size: "{typography.sizeXs}"
---

## Overview

Rentemester skal ligne en moderne dansk bekendtgørelse mere end et SaaS-dashboard. Ro frem for stimulering, dokument-følelse frem for app-følelse, og determinisme er også en æstetisk beslutning. Brugeren skal kunne forstå virksomhed, periode, datagrundlag og konsekvens, før en handling udføres.

Denne fil er den autoritative designkontrakt for cockpit, regnskabsoutput og PDF. [www/DESIGN.md](www/DESIGN.md) gælder marketingwebsitet. YAML-felterne genererer `src/design/tokens.ts`, `app/src/design/tokens.stylex.ts` og CSS-broen via `bun scripts/design-tokens.ts`. Genererede filer redigeres gennem denne kilde; `bun run design:check` afviser drift. Denne fils komponentkatalog og adfærdsregler er fælles; [docs/ui-refactor.md](docs/ui-refactor.md) dokumenterer rutedækning, accept og konkrete verificeringsgrænser.

Regnskabstal og produktdefaults er generelle. Virksomhedsnavne, CVR, kontomappinger, saldi og lokale policyvalg tilhører workspacet. Gallerier og versionsstyrede fixtures bruger syntetiske data.

## Colors

Paletten er varm, papirnær og nøgtern. `paper` er sidefladen, `paper-raised` er formular- og indholdsfladen, `ink` er primær tekst og `ink-muted` er sekundær tekst. Accent bruges sparsomt til udvalgt kontekst, afvigelser og handlinger med konsekvens.

`border` adskiller rapportlinjer og grupper. `border-strong` identificerer aktive inputfelter og sekundære controls. Den stærke kant har mindst 3:1 kontrast mod begge papirflader (ca. 3,11:1 og 3,31:1). Dekorative separatorer behøver ikke samme styrke. Fokus på fælles controls bruger `info`; navigation kan bruge `accent`. Begge skal kunne skelnes fra den tilstødende flade.

Normal tekst skal have mindst 4,5:1 kontrast, stor tekst mindst 3:1. De centrale tekstpar og tekst på statusflader kontrolleres automatisk. `success`, `warning`, `danger` og `info` angiver betydning; status skal altid have en tekst. En farve, et ikon eller en graf-tooltip må aldrig være den eneste kilde til information. Disabled må være afdæmpet, men årsagen til en blokering skal fremgå i nærheden.

## Typography

Overskrifter bruger Source Serif 4 for dokumentautoritet. Brødtekst bruger IBM Plex Sans. Beløb og andre regnskabsnære værdier bruger IBM Plex Mono med tabular figures. Skrifterne leveres lokalt med licenser i produktionsbygningen.

En side har én H1 for den aktuelle opgave; produktnavnet i topbaren er et link. H1 er 32 px, dialogtitel 24 px og brødtekst 16 px med linjehøjde 1,5. Hjælp og tabeller bruger normalt 14 px; 12 px reserveres til korte sekundære labels og grafakser. Lange forklaringer følger `prose-width` på 68 tegn. Semantiske overskriftsniveauer følger dokumentets struktur.

Beløb vises med dansk tusind-/decimalseparator, synlig valuta, minus og nul. Manglende data er `—` med forklaring, når fraværet påvirker en beslutning. Identiteter og regnskabstal må ikke præsenteres som dekorative illustrationer eller trunkeres i afgørende kontrolpunkter. Datoer, id'er og kilder skal kunne kopieres.

## Layout

Layout bruger 4 px som mindste trin og 8 px som grundrytme. De definerede spacing-trin er 4, 8, 12, 16, 24, 32 og 48 px. Ingen lokale afstandsskalaer for den samme komponent. Indholdet har en maksimal bredde på 1440 px med plads til rolige formularer og regnskabstabeller.

### Shell, navigation og kontekst

`CockpitLayout` ejer topbar, workspaceindgange, virksomhedskontekst, navigation og main-landmark. `SkipLink` er synligt ved tastaturfokus og flytter fokus til indholdet. En side har ét aktuelt navigationslink med `aria-current="page"`.

Virksomhedsnavigationens kilde er `app/src/company-route-registry.tsx`; `company-navigation.ts` er en kompatibel projektion. De otte opgaveområder er Status, Kræver opmærksomhed, Penge og bilag, Fakturaer, Moms og frister, Rapporter, Viden og Administration. Tilføj en destination til dette katalog frem for at opfinde en lokal menu. Ruter og permissions fremgår af dækningsmatricen i `docs/ui-refactor.md`.

Fra 1024 px bruges en sidebjælke på 248 px. Under 1024 px åbner den samme navigation i en fælles native dialog med en Luk-handling, som forbliver synlig under menus scrolling. Menuudløseren har aria-haspopup, aria-expanded og aria-controls. Hvert område har et direkte navigationslink. Ekstra sider bruger native `details`/`summary` med en særskilt tekstudløser uden indlejrede links: summary kan nås med Tab og åbnes med Enter/Space; skjulte links deltager ikke i fokusforløbet. Den aktuelle gruppe er åben. Valg af destination lukker menuen, og mobil giver adgang til samme autoriserede opgaver som desktop.

Virksomhed og relevant årsvalg er synlige i den fælles kontekst. Årsvalg opdaterer URL'en. Navigation mellem opgaver bevarer det relevante regnskabsår; filtre fra en liste tilhører den liste. Et returflow bevarer hele dens URL-udsnit. Workspace-, virksomhed-, alle-bilag-, flerårs- og periodescope beskrives efter de faktiske data. Årsvalg må ikke antyde, at en flerårsrapport eller virksomhedsliste er filtreret til ét år.

### Responsive arbejdsmønstre

Ved 320 px skal indhold, formularer og administration fungere uden global vandret scrolling. Handlinger kan ombrydes, og felter stables under 640 px. Brede regnskabstabeller ruller i en lokal, navngivet og tastaturtilgængelig region. Sammenligningskolonner og kontotal bevarer deres relationer.

Bilag, bank, kontakter og fakturaer bruger URL-baserede filtre, sortering og sideskift med 25/50/100 poster, standard 50. Mobil viser de samme records med feltetiketter. Fakturaoprettelse, fakturadetalje, bilagsdetalje og bilagsbogføring har egne deep links. Lange opgaver bruger hele sider med retur til den oprindelige liste.

## Elevation & Depth

Ingen skygger eller glas-effekter i v1. Hierarki skabes med spacing, borders og papirtoner. Dialogens backdrop dæmper baggrunden; browserens modalitet gør den inert. Overlays skal ikke positioneres manuelt oven på en aktiv baggrund.

Bevægelse skal hjælpe orientering. Ingen pulserende tal, dekorativ parallax eller vedvarende animation. Respektér `prefers-reduced-motion`, og lad aldrig animation være nødvendig for at forstå en tilstand.

## Shapes

Former er næsten firkantede. Små afrundinger på 2, 4 og 8 px er tilladt, men ingen pill-buttons. Fælles controls bruger 4 px, statuslabels 2 px. Fokusrammen er mindst 2 px med 3 px afstand og må ikke klippes af en container.

Almindelige controls og navigationslinks har et produktmål på mindst 44 px højde. Kompakte controls kan være 36 px; klikområdet må normalt ikke være mindre end 24 × 24 px. Checkbox/radio har en 20 px indikator med en klikbar label, som giver et tilstrækkeligt samlet mål og afstand til andre handlinger. 44 px er vores komfortmål; WCAG 2.2 AA's minimum er 24 px med definerede undtagelser.

## Components

### Ejerskab og implementering

Cockpit ejer fælles primitives i [app/src/components/ui/index.tsx](app/src/components/ui/index.tsx) og domænekomponenter i `app/src/components`. StyleX kompilerer komponent- og lokale styles; semantiske rapportlayouts og responsive recordmønstre ligger i CSS-laget. Rapporter, CLI-human-output og PDF deler betydning og tokenkilde; de behøver deres egen semantiske rendering.

Komponentgalleriet startes med `bun run --cwd app gallery`. Det viser syntetiske controls, felter, beløb, status og native dialoger og indgår ikke i produktionsruterne. Galleriet er en visuel prøveflade; browsermatricen og komponenttests dækker produktflows.

Kataloget nedenfor omfatter alle offentlige React-komponenter direkte i den fælles komponentmappe og dens UI-entrypoint. Private hjælpere som `TaskLinks` og `CompanyContextHeader` følger shellkontrakten. Types og hooks følger deres komponentfamilie; de er ikke nye visuelle komponenter.

### Fælles primitives

| Komponent | Brug og kontrakt |
|---|---|
| `Button`, `ButtonLink` | Handling bruger native button, navigation link. Varianter: primary, secondary, quiet og danger. Én fremhævet primær handling pr. opgavegruppe. Synlig handlingsetiket, keyboard/fokus, disabled/busy og præcis permission. Danger bruges ved destruktiv konsekvens. |
| `Input`, `Select`, `Textarea` | Native controls med synlig label, autocomplete/inputMode/type efter data, stærk kant og fokus. Placeholder er kun et eksempel. Browserens tastaturadfærd bevares. |
| `Field` | Forbinder label, help og error til ét control med id/for og aria-describedby. En feltfejl markerer aria-invalid og beskriver, hvordan brugeren retter værdien. Flere felter grupperes med fieldset/legend, når de deler et spørgsmål. |
| `MoneyInput`, `Amount` | MoneyInput bevarer rå dansk indtastning; `parseDanishAmount` bruges ved validering. Amount viser valuta og tabular mono. Manglende, negative og nulværdier bevares. Summer aldrig forskellige valutaer. |
| `EntitySelect` | Søg og vælg virkelige konti, bankposter og parter fra autoriserede læsninger. Søgefelt og select har egne labels; det aktuelle valg bevares, selv hvis søgningen skjuler andre muligheder. |
| `PageHeader` | Én H1, kort beskrivelse/scope og relevante handlinger, som ombrydes på mobil. Brugerens opgave står før intern implementering. |
| `FilterBar` | Navngivet filterregion. Mobilknappen har aria-expanded/aria-controls, viser antal aktive filtre og tilbyder Ryd filtre. Filtre må ikke sende mutationer. |
| `Pagination` | Navngivet navigation med antal poster, position og 25/50/100-valg. Forrige/Næste deaktiveres ved grænser. Et filter- eller størrelsesskift genberegner en gyldig side. |
| `DataTable` | Semantisk caption, th/scope og stabile row keys. `TableColumn` beskriver id/label/render. Lokal navngivet scrollregion; beløb bevarer hele værdien og valuta. Tilføj ikke interaktiv rolle til almindelige dataceller. |
| `StatusBadge` | Tekstlabel og neutral/success/warning/danger/info. Vis status, aldrig en handling. Farve må ikke stå alene; ingen unødvendig live announcement for statiske badges. |
| `ResultReceipt` | Rolig statuskvittering med serverens faktiske resultat, identitet og næste handling. Oprettet, bogført og leveret er forskellige tilstande; vis kun det, serveren har bekræftet. |
| `Dialog` | Fælles native modal eller mode=page. Titel navngiver dialogen og ombrydes ved lange ord. Valgfrit id forbinder udløserens aria-controls; initialFocusRef styrer et passende startpunkt; returnFocusRef bevarer udløseren ved asynkron åbning. `evidenceTaskOutcome` markerer den synlige åbningstilstand til kandidatens browserkontrakt. Adfærden følger dialogreglerne nedenfor. |
| `FormField` (`CockpitPrimitives`) | Kompatibilitetsadapter for eksisterende label, hint og fejlkontrakter; native controls bruger de fælles StyleX-primitives. |
| `FilterBar` (`CockpitPrimitives`) | Bevarer aktive filtre, nulstilling og progressive avancerede filtre med eksplicit evidensmærkning. |
| `PageState` | Loading, tom, warning, blocked og error med navngivet overskrift og relevant retry/handling. Error/blocked annonceres som alert. |
| `StatusChip` | Tekst og tone; coverage viser serverens datadækning uden at beregne ny status. |
| `PageHeaderActions`, `MetricCard` | Ombrydende handlinger og nøgletal med synlig label; manglende beløb er fortsat manglende. |
| `ResponsiveTable` | Bevarer native tabeller, navngivning og lokal scrollregion; data-label støtter mobil detaljevisning. |
| `LegacyBankBindingModal`, `LegacyPayableBackfillModal` | Eksakt kildeidentitet, særskilt planreview og bekræftet apply. Ingen beløbsmatch eller remapping; serverens planhash og idempotency bevares. |
| `PartyLink`, `PartySummary` | Kun eksplicit canonical party-id giver et profil-link. Navn og roller er læsende kontekst, aldrig en udledt partsbinding. |

### Shell og navigation

| Komponent/familie | Brug og kontrakt |
|---|---|
| `CockpitLayout`, `SkipLink` | Fælles landmarks, topbar, menu, main og tastaturgenvej til indhold. Auth-/workspace-/virksomhedskontekst skal kunne forstås på alle viewportstørrelser. |
| `CompanyNavigationShell`, `CompanyNav` | Samler virksomhedens navn, år og scope ét sted. Viewets kontekst opdaterer shellen uden at duplikere navigationen. |
| `CompanyTaskNavigation` | Grupperet navigation ved brug uden den fælles shell; samme rutekatalog og adgangsregler. Ingen lokal alternativ taksonomi. |
| `YearSelector` | Synligt regnskabsår med autoriserede år og arkivmærkning. `useCompanyYear`, `useCompanyShell` og `accountPostingsTo` bevarer den dokumenterede kontekst og kontodrill-down. |
| `CompanySwitcher` | Viser kun serverautoriserede medlemskaber med navn og rolle. Virksomhedsskift nulstiller fremmede data og bevarer kun relevant kontekst. |
| `AccountMenu` | Identitet og workspace-rolle, logout, sessioner og adgangskode. Sessioner/adgangskode bruger Dialog med udløserrelation, fokusretur og lukning. Revocation og logout på alle enheder kræver bekræftelse. Tokens og rå user agents vises aldrig. Adgangskoder slettes fra felterne ved lukning, succes og fejl og gemmes aldrig i browserlager. |

### Formularer og domænedialoger

| Komponent/familie | Brug og kontrakt |
|---|---|
| `ConfirmDialog` | Forklar præcis konsekvens, relevant identitet og evt. begrundelse/modtager før eksplicit handling. Destruktive bekræftelser starter på Annullér. Busy, serverfejl, lås og ukendt udfald følger de fælles regler. |
| `BankImportModal`, `ImportModal` | Importprofil/kilde og fil før import, serverens resultat bagefter. Filer, datagrundlag og fejl beskrives; import må ikke omtales som fuldført før et bekræftet svar. |
| `BankReconcileModal`, `BankCorrectionModal`, `DirectBankPayableCorrectionModal` | Viser valgte bankposter, bilag og korrektionens konsekvens. Bevar serverens plan-/revision-/populationskontrol og eksplicit godkendelse. |
| `ContactFormModal` | Navn, partsrolle og stamdata med separate labels. Eksternt opslag er en eksplicit handling med egen permission; opslagssvar er forslag, indtil de gemmes. |
| `DocumentIngestModal` | Kilde, fil og evt. kontekst ved indlæsning. Vis registreret dokumentidentitet og næste trin; indlæst er ikke det samme som bogført. |
| `DocumentBookExpenseForm`, `DocumentBookExpenseModal` | Samme bogføringsform/validering på hel side og i kompatibilitetswrapper. Bilag, bank, konto, moms og valuta er synlige før bogføring. Sideflow er standard for den lange opgave. |
| `InvoiceIssueForm`, `InvoiceIssueModal` | Samme fakturaform i sideflow og wrapper: kunde, dato, linjer, valuta, moms, preview og udstedelseskvittering. Udstedelse og ekstern levering har hver sin konsekvens og permission. |
| `PayableRegisterModal`, `MileageRegisterModal` | Registrering med dokumenteret dato, part/formål, beløb eller distance og serverens kvittering. Fejl bevarer brugbare felter. |
| `RecurringInvoiceTemplateModal` | Kundedata, linjer og gentagelsesplan gennemgås før oprettelse. En skabelon er ikke en udstedt faktura; senere generering har sin egen kvittering. |

### Feedback, status og datagrundlag

| Komponent/familie | Brug og kontrakt |
|---|---|
| `Loading` | Kort, specifik label i role=status (polite). Vis kun en igangværende læsning som indlæsning; fokus flyttes ikke. |
| `ErrorState` | Forklaring i role=alert og evt. Prøv igen for en læsning. Ingen automatisk gentagelse af mutationer. |
| `Banner` | Error annonceres som alert; success/warning som status. Placer beskeden ved den relevante opgave. Beskriv betydning og en mulig næste handling. |
| `ArchivedBanner`, `LockBanner` | Forklar arkiv/lås, scope og tilladt næste trin. En lås må ikke fremstilles som en indtastningsfejl. Serverens periode-/backupkontrol er afgørende. |
| `UnknownMutationNotice` | Ukendt udfald blokerer ny skrivning. Tilbyd læsende statuskontrol og særskilt menneskelig afklaring; bevar blokeringen ved genåbning/reload i samme fane. Lagerfejl vises og blokerer skriveruter. |

En tom liste forklarer, om der mangler data, eller om filtrene udelukker poster. Tilbyd en relevant indlæsning eller Ryd filtre, når det er tilladt. Tom, loading, fejl, arkiv, lås og ukendt udfald er forskellige tilstande. Statiske forklaringer skal ikke gentagne gange afbryde skærmlæseren.

Læsesvar tilhører virksomhed, dokument og periode. Gamle data skjules ved kontekstskift; læsninger kan afbrydes. En fejl ved statusopdatering vises ved det seneste kendte datagrundlag. UI må ikke lade gamle tal se aktuelle ud. En mutation gentages aldrig automatisk. Netværksbrud eller ulæseligt svar vises som ukendt resultat med synligt servergrundlag. En læsning frigiver aldrig automatisk handlingen; brugeren kontrollerer resultatet og afklarer blokeringen udtrykkeligt. Bekræftet succes og kendt valideringsafvisning behandles særskilt.

### Grafer og andre fælles produktflader

| Komponent/familie | Brug og kontrakt |
|---|---|
| `PnlChart` | Månedlige indtægter/udgifter med navngivet canvas og native fold-ud-tabel med de samme værdier. Valuta følger den viste rapport. |
| `CashflowChart` | Ind-/udbetalinger og banksaldo med navngivet canvas, eksplicit valuta og reference til månedstabellen, som også viser banksaldo. Null er manglende saldo, ikke nul. Saldi summeres aldrig mellem måneder. |
| `MultiYearChart`, `MultiYearBalanceChart` | Navngivet canvas med aria-details-reference til den fulde resultat-/balancetabel. År vises i rækkefølge; delvist live-år mærkes år til dato i både graf og tabel. |
| `chartCurrency`, `CHART_AXIS_NUMBER` | Fælles dansk tooltip-/akseformat. Tooltip har rapportens valuta; kort akseformat må kun bruges, når grafens enhed er tydelig. Legend er 14 px, akser 12 px fra tokens. `useChartFonts` indlæser de lokale graffonts og gentegner uden animation, så canvas ikke fastholder fallback-skrift ved første besøg. |
| `CompanyCard` | Virksomhedsidentitet, regnskabsscope, nøgletal og opgaver med semantiske links. Manglende ledger eller delvis dækning forklares før aggregerede tal. |
| `CompanyForm` | Fælles stamdata ved onboarding, oprettelse og administration. Felter og defaults er generelle; kontekst, validering og adgang bestemmer det konkrete workflow. |
| `AccountantExportCard` | Forklar eksportens indhold, periode og modtagerformål; brug præcis export-permission. Download er en handling med sin egen status og fejl. |

Grafer forklarer mål, enhed, periode og kilder. Legend og tabel gør serier forståelige uden farvesyn eller hover. Den tilgængelige tabel skal bevare samme population, manglende data, fortegn, valuta og periode som grafen. Integrerede grafer genbruger en eksisterende rapporttabel; en selvstændig graf medbringer sin egen tabel. Hvis en graf bruges uden den tilsvarende tabel, skal den tilføjes ved samme integration.

### Dialog- og formularadfærd

Brug en dialog til et afgrænset valg eller en kort formular; brug en hel side til lange arbejdsgange og varige deep links. `mode=page` har normal sidehøjde og intet modal-fokusforløb. Lange formularer opdeles efter brugerens opgave, med native details til valgfri uddybning.

En modal åbnes med `showModal()`, har en synlig titel og en synlig Luk/Annullér-handling. Startfokus går til det første meningsfulde felt eller en eksplicit ref; ved destruktiv handling til Annullér. Tab/Shift+Tab bliver i den øverste dialog og inkluderer summary, links og åbne controls; skjulte, inerte og disabled controls springes over. Fokus går tilbage til udløseren, hvis den stadig findes; brug returnFocusRef, hvis en asynkron læsning midlertidigt deaktiverer den. Positive tabindex og hjemmelavede focusable div-knapper bruges ikke.

Escape og backdrop beder om lukning. Busy blokerer lukning og gentagelse, indtil handlingen har et udfald. Ugemte forretningsfelter beskyttes ved Cancel/Escape/backdrop, route-/år-/virksomhedsskift og browserlukning. Succes markerer formularen gemt før retur. Passwordfelter er en bevidst undtagelse: udtrykkelig lukning rydder de følsomme værdier. En ekstra dialog bruges kun til en nødvendig bekræftelse, fx kassering eller sessionafslutning; kun den øverste ejer tastaturet.

Formularer beskriver obligatoriske felter og enheder. Kendt valideringsafvisning bevarer indtastningerne og placerer en forståelig fejl ved opgaven/feltet. API- og regnskabsfejl må ikke skjules af optimistisk succes. Controls grupperes med native semantik. En visningsvælger bruger knapper med aria-pressed; role=tab kræver det fulde tab-/tabpanel-/piletastmønster.

### Vedligeholdelse og accept

En ny eller ændret fælles komponent opdaterer denne kontrakt i samme ændring. Genbrug en eksisterende primitive først; udtræk et fælles mønster, når faktisk gentagelse eller adfærd kræver det. Ingen katalogkomponenter uden et brugsscenarie. Tokenændringer genereres, og fælles komponenters dokumentationsdækning og aktive control-/fokuskontraster kontrolleres automatisk.

Relevante tilstande kontrolleres i komponenttests og i den faktiske browser/CSS: tastatur, modalitet/fokus, permissions, loading/error/empty, validering, usaved-guard, ukendt udfald, danske beløb, responsive layouts og rigtige serverkontrakter med syntetiske fixtures. Designreview vurderer først forståelighed og korrekt opgaveløsning, derefter visuel konsistens. Automatisk axe-kontrol suppleres med manuel skærmlæser og faktisk 200 % zoom; de udførte checks og resterende kontrolpunkter registreres i `docs/ui-refactor.md`.

## Do's and Don'ts

Do: brug varme papirfarver, faste spacing-trin, native semantik og tabular mono til beløb. Skriv handlingens konsekvens i brugerens sprog. Bevar kilder, datagrundlag og tilladte næste trin.

Don't: brug emoji, gradients, chatbobler, mørkt tema i v1, pill-buttons, pynt på regnskabstal, farve alene som status eller interne kodebegreber i almindelige opgaveflows. Gem ikke kritiske handlinger i hover. Opfind ikke et nyt modal-, filter-, menu- eller beløbsmønster lokalt.

Adgang til en workspace giver ikke automatisk ejeradgang til dens virksomheder. UI skjuler handlinger, der ikke er tilladt, men serveren afgør altid adgang, periodekontroller, revisionsgrundlag, planhash og bogføring. Automatisk browserkontrol er ikke en attest for fuld WCAG-overholdelse.

Reglerne om kontrast, klikmål og modalitet bygger på [W3C: Non-text Contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html), [W3C: Target Size (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html) og [WAI-ARIA: Modal Dialog Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/). Strukturerede graftabeller forbindes med [WAI-ARIA: aria-details](https://www.w3.org/TR/wai-aria-1.2/#aria-details), så tabelstrukturen bevares. Produktspecifikke valg som 44 px controls, varm palette og lange sideflows er Rentemesters egne designbeslutninger.
