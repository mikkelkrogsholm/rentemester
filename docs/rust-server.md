# Kør Rust-serveren med web-UI'et

Rust-serveren leverer det eksisterende React-cockpit og behandler selv login,
ejeroprettelse, MFA, sessions, virksomhedsadgang, manuel bogføring, tilbageførsel,
virksomhedsstamdata, kontooprettelse, bekræftelse af kontoroller, bankkonti, generisk bankimport, bilagsbogføring med valutakøb samt bilagsindlæsning og resultatopgørelse, balance og saldobalance med CSV/PDF-eksport samt momsoverblik med PDF, kundeoprettelse, udstedelse/bogføring af danske salgsfakturaer samt betalinger, kreditnotaer, afstemning af kundetilbagebetalinger og leverandørkort med bilagsudfyldning samt leverandørgæld med senere bankbetaling. Den
starter ikke TypeScript-backenden og sender ikke forespørgsler videre til den.

## Aktuel Rust-levering

Den fælles bilagsindbakke kører nu gennem Rust-server, web, CLI og MCP.
Originalen kan visiteres til en eksplicit virksomhed og afleveres til dens
bilagsregister uden en bogføringspost. Faktiske web- og klientforløb med
genstart består. Signeret backup, uafhængig verificering efter fjernelse af
kildedata og privat nøgle samt gendannelse med en ny ejer og MFA består også;
originaler, historik og målbilag bevares.

Selskabsdokumenter har Rust-understøttelse for immutable originaler,
metadata, typed links og begrundede erstatninger. Web, CLI, MCP og native
skærm er integreret. Den signerede backupkode bevarer alle fire tabeller og
originalbytes, inklusive ældre links uden efterfølgende scope-udsagn.
Det faktiske web- og CLI/MCP-forløb med genstart består nu, inklusive
originaldownload og fuld historik. Alle tre arkiver fra web, CLI og MCP er
også verificeret uafhængigt efter fjernelse af kilde og privat nøgle og hver
gendannet med ny ejer, normal MFA og genstart. Originaler, fuld historik,
scope og følsomhed bevares. Ældre links uden scope og læsning med reducerede
roller er her dækket af unit-tests. De 17 storage- og host-tests består.

Ejer- og kontrolforhold har nu Rust-forslag, review af en anden autentificeret
person og særskilt apply af de præcise snapshot- og diff-hashes. Senere
daterede observationer bevarer den oprindelige historik. Web, CLI, MCP og
native skærm bruger serverens fælles regler. Det første faktiske forløb med to normalt inviterede MFA-brugere består:
forslag, uafhængigt review, apply, dateret efterfølger og CLI/MCP-paritet.
Hele web- og CLI/MCP-forløbet består også efter genstart med frisk MFA,
inklusive de fulde hashes, historik, originaler og daterede ejerfacts. Den
konkrete visningsfejl var fem samtidige registerlæsninger mod fire
serverpladser; den begrænsede kapacitet er nu tilpasset til otte.

Mellemregningsafstemning bruger nu en eksplicit koncernkonfiguration og
kontomappinger, som godkendes af en anden autentificeret ejer. Rust læser de
oprindelige posteringer og kildebeviser fra begge selskaber og viser eksakte
beløb og differencer. Handlingerne ændrer konfiguration og revisionsspor;
hvert selskab beholder sine egne posteringer. Web og seks CLI/MCP-operationer
er integreret. Faktiske første forløb og genstart består i separate workspaces
for web og klienter med almindelig invitation, verificering og to forskellige
MFA-brugere. Forkerte hashes, selv-godkendelse, overlappende aktive mappings
og bortfald af adgang afvises.

Konsolidering har nu eksplicitte rapportprofiler, uafhængig godkendelse og
rapporter med selskabernes kildebeviser. Elimineringer har særskilt plan,
godkendelse, apply og tilbageførsel, inklusive begge mellemregningsretninger.
Funktionerne er integreret i Rust-serveren og web-UI’et. Konsolideringsforløb
i separate web- og CLI/MCP-workspaces består med to MFA-brugere og genstart:
rapportprofiler, uafhængig godkendelse og tilbagekaldelse, eksakte beløb og
kildebeviser samt uændrede selskabsposteringer. Det samlede forløb med
elimineringer i begge retninger består også før og efter genstart: plan,
godkendelse, apply, tilbageførsel og en balanceret konsolideret rapport.
Forældede kildebeviser, selv-godkendelse, dobbelt apply og bortfald af
adgang afvises. Alle otte særskilte CLI/MCP-handlinger består også i et
separat workspace med begge retninger, uafhængig godkendelse, fuld paritet
med HTTP-svarene og uændrede posteringer før og efter genstart.

Der er 186 verificerede MCP-tools efter kontaktimporten i Group60.
De samlede native tests består med 191 beståede og én eksisterende ignoreret
test; native build består. Mac’en er låst op. Administreret
lokal opstart, almindelig ejeroprettelse, verificering, MFA og læsning af
saldobalancen er gennemført i det native vindue. Fakturering og udgifter er
endnu ikke godkendt i desktop. Efter oplåsningen er også tastaturbetjening
og en konkret fokusrettelse prøvet; navigationen i det faktiske vindue er
fortsat uverificeret. De afgrænsede forsøg er stoppet, og
kun den private syntetiske profil er bevaret til senere accept.

Native **Mellemregningsafstemning** er også integreret og består tests/build.
Native **Konsolideret rapport** og **Elimineringer** er også integreret med
tests og build.
Faktisk betjening af de nye skærme i desktop er endnu ikke godkendt.
Backupkoden for ejerforhold bevarer tidligere og aktuelle snapshot-, review-,
fact- og sluthistorikker. Alle tre arkiver fra web, CLI og MCP er verificeret
efter fjernelse af kilde og privat nøgle og hver gendannet med ny ejer, MFA
og genstart. Signeret backup
understøtter også den eksplicitte V1-koncernkonfiguration og mappinghistorik.
Konsolideringsprofiler og elimineringer har også et fuldt signeret format
med historik og begge payloadversioner. Alle tre arkiver fra web, CLI og MCP
er verificeret efter fjernelse af kilde og privat nøgle og hver gendannet med
ny ejer, MFA og genstart. Den fulde koncernrapport og kildebeviser bevares.
En efterfølgende ny postering blokerer korrekt de forældede
elimineringsbeviser, også efter endnu en genstart. Ikke-tomme dispositioner
bevares nu af Intercompany-format version 2 med originale begivenhedskæder,
posteringstilknytninger og journalbeviser. Den samlede Group59-prøve med
genstart og signeret gendannelse hos en ny ejer består.

Planlagte fakturaer har en varig kø med eksplicit samtykke, pause,
genoptagelse, annullering og gennemgået catch-up eller spring-over. Serveren
kontrollerer den aktuelle bruger ved hver udstedelse. Webappen kan efter
aktivt valg køre dagens forekomst, mens den er åben; manglende forekomster,
uafklarede svar og bortfald af adgang stopper automatikken til gennemgang.
Udstedelse danner fakturaoriginal og PDF, uden bogføring eller afsendelse.
Status og genåbning starter ingen kørsler. Det faktiske web-/klientforløb med to MFA-brugere består også efter genstart:
fire udstedte originaler og PDF’er, præcis kø og spring-over, korrekt
adgangsafvisning og et accepteret kald med mistet svar uden gentagelse.
Alle fire fakturaer er ubogførte og usendte; selskabsposteringerne er uændrede. Desktopappen har nu også automatisk kørsel i den aktive app-session efter
gennemgået aktivering eller særskilt bekræftet start. Kørslen fortsætter ved
skærm- og virksomhedsskift; nyt login, ændret forbindelse og Stop ophæver
den lokale aktivering. Manuelle og automatiske handlinger deler én spærring.
Uafklarede forsøg bevares til manuel kontrol, også efter genstart, og succes
kræver genlæsning af den præcise faktura og dens PDF. Native tests og bygning
består; dette nye fakturaforløb er endnu ikke verificeret i det faktiske
desktopvindue på grund af den dokumenterede klik-/scrollbegrænsning.

Signeret backup bevarer nu hele planlægningshistorikken og fakturaernes
JSON-originaler og PDF’er. Ved gendannelse er tidligere samtykke og køvalg
inaktive. En aktuel ejer skal gennemgå skabelonen og give nyt samtykke;
godkendelsen starter ingen automatik. Historiske køvalg skal vælges igen,
mens allerede udstedte, oversprungne eller afsluttede fejlperioder fortsat
er afgjort i planlægningen. Uafklarede forsøg bevares og blokerer nyt samtykke.

Det faktiske forløb består for tre uafhængige arkiver fra web, CLI og MCP:
verifikation efter fjernelse af kilde og privat nøgle, ny ejer med normalt
MFA-login, gennemgået samtykke i alle tre klienter, ét nyt køvalg og én
udstedelse med JSON/PDF samt genåbning. Gentaget udstedelse af samme periode
afvises; fakturaerne er ubogførte og usendte. Gentagen backup efter nyt
samtykke og gendannelse af uafklarede forsøg er dækket af signerede
lagertests, ikke dette browserforløb. Native samtykkekontroller består
tests og bygning; deres faktiske GUI-forløb er stadig uverificeret.

Serverens eksplicit aktiverede fakturaarbejder er nu integreret med servicekonti,
versionsfast samtykke og separat audit for servicehandlinger. Det faktiske
forløb med normal MFA, weboprettet servicekonto og CLI/MCP-kontrol består:
lukket browser giver præcis én ubogført og usendt faktura med JSON/PDF;
genstart gentager den ikke. Et signeret V3-arkiv er verificeret efter
fjernelse af kilde og privat nøgle og gendannet med en ny ejer og normal MFA.
Historik, originaler og ledger-identitet bevares; tidligere serviceadgang, kø
og serversamtykke er inaktive, og ingen ny faktura udstedes. Eksterne
leverancer, øvrige hostede arbejdere og administreret flytning mellem lokal
app og server er fortsat uafsluttede dele af målet.

Den samlede omskrivning er fortsat åben. De næste nødvendige dele er
varige eksterne handlinger, øvrige hostede arbejdere og administreret flytning
mellem lokal app og server.
Ejerobservationer bliver ikke automatisk til en koncernkonfiguration.

Det almindelige HTTPS-loginforløb består nu også i en isoleret kontrol med
privat TLS-proxy og mailgateway: første ejer verificeres og opsætter MFA,
en anden bruger inviteres gennem web og verificeres med egen MFA. Et
reset-link virker, selv når gatewayen har modtaget mailen og svaret går tabt.
Tidligere session, adgangskode og brugt reset-token afvises; nyt login med
eksisterende MFA og fuld genstart består. Der sendes ingen mail til virkelige
modtagere i denne kontrol. Produktionens gateway og TLS-proxy kræver deres
egen driftsverifikation.

## Prøv med syntetiske data

Fra repository-roden, med Docker og Bun samt projektets afhængigheder installeret:

```sh
python3 scripts/rust/run-web-demo.py
```

Launcheren bygger cockpittet og Rust-serveren, opretter et isoleret workspace
med en eksplicit syntetisk kontoplan og viser et privat installationslink.
Åbn installationslinket og opret din demo-ejer i browseren. Launcheren viser også
kommandoen til at læse det lokale bekræftelseslink efter oprettelsen.

I browseren:

1. Opret den første ejer via installationslinket, og åbn e-mailbekræftelseslinket.
2. Log ind og opsæt totrinsbekræftelse. Gem recovery codes.
3. Vælg virksomheden med **Åbn bogføring**.
4. Vælg **Ny postering**, indtast tekst og beløb, og vælg debet- og kreditkonto.
5. Gennemse og bekræft posteringen.
6. Åbn **Saldobalance** og kontrollér debet, kredit og saldo.

Demoen har en bankkonto i kontoplanen, egenkapital og en udgiftskonto. Den
manuelle åbningspostering bruger bank og egenkapital; udgifter bogføres med
bilag og en importeret bankbetaling som beskrevet nedenfor.

Ctrl-C stopper demoen og fjerner dens container og datavolume. Kun en separat
Rust-buildcache bevares. Eksisterende virksomhedsdata bruges ikke.

## Åbn og hent rapporter

Under **Rapporter og planlægning** kan du åbne **Resultatopgørelse**, **Balance**
og **Saldobalance**. Vælg regnskabsår og brug **Hent CSV** eller **Hent PDF** på
rapportsiden. **Posteringer** kan også hentes som CSV, inklusive et valgt kontofilter.

Sammenligningstal følger det valgte regnskabsår. Balancen viser «—», når der
ikke findes et tidligere år i ledgeren. Momskonti placeres i aktiver eller
passiver efter deres normale debet-/kreditside. PDF-filer med mange kontolinjer
fordeles over flere sider. CSV bruger UTF-8, semikolon og danske decimalkommaer.

## Indlæs og åbn et bilag

Åbn **Bogføring → Bilag → Indlæs bilag**. Vælg en fil, bilagstype og kilde,
og udfyld oplysningerne. Ved **Kassebon** kan du eksempelvis angive dato,
bruttobeløb og momsbeløb. Indlæs bilaget, luk kvitteringen, og vælg **Åbn bilag**
på bilagslisten for at se originalfilen.

Rust gemmer originalen med indholdshash og registrerer metadata, bilagsnummer,
opbevaringsfrist og aktør. Filtypen kontrolleres, eksisterende originaler
overskrives ikke, og identisk filindhold afvises som dublet. Originalfilen kan
kun læses med virksomhedens bilagsadgang og efter hashkontrol. Bilaget og
originalfilen bevares ved servergenstart.

Bilagsindlæsning alene bogfører ingen udgift. Tvungen indlæsning og
leverandørkobling afvises eksplicit, indtil de tilsvarende forløb er porteret.

## Importér en bankbetaling og bogfør bilaget

1. Indlæs en kassebon med dato, beløb 250 kr., moms 50 kr. og valuta DKK.
2. Åbn **Virksomhedsadministration → Bankkonti**, og opret en bankkonto.
   I demoen kobles den til ledger-konto **1800**. Notér kontoens slug fra listen.
3. Åbn **Bogføring → Bank → Importér kontoudtog**. Vælg en UTF-8-CSV,
   angiv bankkontoens slug, og lad bankformatet være tomt. Brug bilagets dato:

   ```csv
   dato;tekst;beløb;valuta;saldo
   2026-10-18;Kontorartikler;-250,00;DKK;-250,00
   ```

4. Åbn bilaget og vælg **Bogfør bilag**. Vælg demoens udgiftskonto **3000**
   og bankbetalingen. Vælg **Ikke fradragsberettiget**, hvis momsen skal indgå
   i udgiften, og bekræft bogføringen.
5. Bankposten vises nu som afstemt. Resultatopgørelsen viser en udgift på
   250 kr. og et resultat på −250 kr., hvis der ikke findes andre indtægter
   eller udgifter. Originalbilaget og forbindelsen til bankposten bevares.

CSV-importen accepterer danske kolonnenavne, semikolon, komma eller tabulator,
komma-decimaler og citerede felter med linjeskift. Brug ISO-datoer for at undgå
at gætte mellem dag og måned. Genimport springer dubletter over, men to ens
betalinger i samme fil bevares som to forekomster. Den originale importtekst
gemmes uforanderligt under `bank/processed/` med indholdshash.

CSV-kolonnen **saldo** (også `balance_after`, `balance` eller `running_balance`)
gemmer bankens oplyste saldo efter den enkelte bevægelse. Med eksemplet ovenfor
viser **Bank** først faktisk saldo −250 kr., bogført saldo 0 kr. og difference
250 kr. Efter bogføring vises **Kontoudtog og bogført saldo stemmer: −250 kr.**
Bankens saldo beregnes aldrig fra bogføringen. Den bogførte saldo indeholder
også modposteringer fra tilbageførsler.

Slutsaldoen bestemmes ud fra sammenhængen mellem beløb og løbende saldi på den
seneste bankdato; filens rækkeorden afgør den ikke. DKK-saldi fra flere bankkonti
summeres kun, når alle konti har et entydigt grundlag. Manglende saldi,
modstridende rækkefølge, blandede tilknyttede/ikke-tilknyttede konti eller en
kontos saldo i fremmed valuta vises som uafklaret med en forklaring. Når ingen
saldo-kolonne er leveret, vises fortsat ukendt kontoudtogssaldo. En postering i
fremmed valuta på en DKK-bankkonto bruger det dokumenterede DKK-beløb ved
kontrollens saldoforløb; bankkontoens valuta bestemmer saldoens enhed.

Genimport med samme saldo springes over som en dublet. En ændret eller senere
tilføjet saldo på samme transaktionsidentitet afvises, så den oprindelige
bankevidens ikke overskrives eller bliver til en ekstra betaling. Rettelse af
den type kildekonflikt kræver fortsat et særskilt korrektionsforløb.
Bankkontrollen er
et grundlag for dette forløb. Review og almindelig periodelåsning er nu porteret;
tvangslukning mangler stadig. Eksterne indberetningskvitteringer kan registreres fra **Perioder og låsning**.

Udgiftsbogføring understøtter DKK og valutakøb med standardkøbsmoms, momsfri
behandling eller ikke-fradragsberettiget moms. Manglende kontomoms kræver et
aktivt valg. Standardkøb kræver momsregistrering, dokumenteret dansk
leverandør, korrekt 25%-grundlag og en aktiv, bekræftet købsmomskonto.
Originalens hash og de eksisterende dato-, periode- og journalkontroller
kontrolleres ved bogføring. En allerede afstemt bankpost kan ikke bruges igen.

Navngivne bankprofiler, omvendt betalingspligt, EU-varekøb,
repræsentation, forenklede fakturaers virksomhedskontekst og automatisk
oprettelse af undtagelsesopgaver mangler fortsat i Rust og afvises eller
udelades eksplicit. De er del af den resterende omskrivning.

## Opsæt og bogfør et dansk køb med momsfradrag

1. Åbn **Virksomhedsadministration → Administrér** og gem virksomhedens
   adresse og momsperiode under **Virksomhedsoplysninger**.
2. Åbn **Kontoplan → Tilpas kontoplan → Opret konto**. Opret en momskonto
   med type **Moms**, normal saldo **Debet** og dit valgte kontonummer.
3. Under **Bekræft kontorolle** vælger du **Købsmoms**, vælger momskontoen
   og bekræfter. Rollen kræver en aktiv konto med passende type og normal saldo.
4. Opret om nødvendigt en udgiftskonto med type **Omkostning**, normal saldo
   **Debet** og standard momskode `DK_PURCHASE_25`.
5. Indlæs en dansk købsfaktura med dato, leverance, afsender og modtager med
   adresser og momsnumre. Eksempelvis 125 kr. inklusive 25 kr. moms.
6. Importér bankbetalingen på −125 kr. og vælg **Bogfør bilag**. Vælg
   udgiftskontoen og betalingen. Kontomomskoden kan udlede standardbehandlingen.
7. Resultatopgørelsen viser 100 kr. i udgift; balancen viser 25 kr. købsmoms
   og bankens fald på 125 kr. Betalingen er afstemt.

Kontonumre og roller vælges eksplicit i workspacet. Eksisterende kontonumre
overskrives ikke. Bekræftede rolleændringer versionsgemmes med aktør og auditspor.
Åben momsaktivitet blokerer afregistrering; momsperioder og aktivitet blokerer
et direkte skift af momsperiode, som kræver et særskilt dateret migrationsforløb.
Adresse og momsændringer gemmes samlet, så en afvist ændring ikke efterlader
halvt opdaterede stamdata. Ændring af allerede eksisterende betalingsoplysninger,
CVR-opslag, omdøbning og arkivering er endnu ikke porteret i dette forløb.

## Se købsmomsen i momsoversigten

Efter købet ovenfor åbnes **Moms og perioder → Moms**. Købet på 125 kr.
viser 25 kr. købsmoms og et momstilsvar på −25 kr., hvis perioden ikke har
anden moms. Oversigten bruger virksomhedens registrerede måneds-, kvartals-
eller halvårsmoms. Den vælger den tidligste aktive, ikke indberettede periode;
ellers vises den aktuelle periode. Perioder med nul moms, men bogføringsaktivitet,
kan også kræve opmærksomhed.

Tallene er foreløbige i en åben periode. Kopiering til en momsangivelse er
slået fra. **Hent PDF** gemmer samme periode, frist, rubrikker og status som
siden. Udskriften er et arbejdsgrundlag og markerer, når den ikke kan indberettes.
Gennemgået periodelukning og genåbning er nu tilgængelige i Rust-cockpittet;
tvangslukning mangler stadig.

Momsberegningen bruger præcise ørebeløb, kontotyper, bekræftede kontoroller og
posteringernes eksplicitte momskoder. Kontoens aktuelle standardmomskode
omklassificerer ikke tidligere posteringer. Originaler og tilbageførsler indgår
på deres egne datoer. Overførsler mellem momskonti og den bekræftede
afregningskonto skaber ikke ny moms. Uforklarede momsbeløb, uoverensstemmende
momsgrundlag og historiske importer, der kræver endnu ikke porteret kildevalidering,
blokerer en færdig momsangivelse. Der gættes ikke ud fra kontonumre.

## Bogfør et valutakøb

Indlæs eksempelvis en kassebon på **100 EUR**, og angiv beløb, valuta og
bilagsdato. Importér derefter betalingen via bankdialogen. Rust understøtter
to dokumenterede omregningsgrundlag:

- En DKK-betaling på **−746,25 DKK** for bilagets 100 EUR. Hvis bankposten
  ikke angiver en kurs, udleder og gemmer Rust kurs **7,462500** fra betalingen.
- En EUR-bankpost på **−100 EUR**, der angiver **−746 DKK** og kurs **7,460000**.
  Her bruger Rust bankens importerede kurs.

Eksempel på de to bankposter i samme generiske CSV:

```csv
date;text;amount;currency;amount_dkk;fx_rate_to_dkk
2026-10-18;EUR-køb betalt i DKK;-746,25;DKK;;
2026-10-18;EUR-køb med bankkurs;-100;EUR;-746;7,46
```

Vælg bilaget, betalingen, udgiftskontoen og den passende momsbehandling i
**Bogfør bilag**. Med **Ikke fradragsberettiget** indgår hele betalingen i
udgiften, inklusive eventuel udenlandsk lokal moms. Den bliver ikke til dansk
købsmoms. Journalen bevarer bilagets valuta, det positive originalbeløb,
DKK-værdien og den positive seks-decimale kurs. Rapporter og bankafstemning
bruger DKK-beløbet.

En importeret eller udledt kurs skal kunne genskabe den dokumenterede
DKK-betaling til øren. Rust afviser uforenelige beløb eller valutaer og kurser,
hvor seks decimaler ikke er tilstrækkeligt. Der hentes ingen udokumenteret
markedskurs. Ved blandet købsmoms fordeles en eventuel afrundingsrest på en
momsfri nettolinje, når der findes en, så totalen stadig svarer til betalingen.

## Gennemgå, luk og genåbn en momsperiode

Forløbet låser bogføringsperioden og gør momsrapportens rubrikker klar som
angivelsesgrundlag. Det udfører ingen indberetning til en myndighed og opretter
ikke en bankbetaling eller en momsafregningspostering.

1. Afstem bankens importerede kontoudtog og bekræft kontorollerne **Bank**,
   **Debitorer** og **Kreditorer**. Kontoudtogets uafhængige saldo skal stemme med
   bogføringen; ved manglende eller tvetydig bankevidens kan perioden ikke lukkes.
2. Åbn **Moms**, vælg et år med en afsluttet momsperiode, og vælg
   **Luk momsperiode → Luk perioden**. UI'et henter kontrollerne, gemmer et
   eksplicit review og sender dets hash og id til serveren.
3. Serveren genberegner kontrollerne i samme transaktion som periodelåsen.
   Et forældet review, uafstemte bankposter, ukendt kontroldokumentation,
   blokerende undtagelser eller ufuldstændige bogføringsbatcher afviser lukningen.
   En momsperiode skal være afsluttet og passe nøjagtigt til virksomhedens
   registrerede momsperiode. Almindelig lukning omgår ingen kontrol.
4. Efter lukning viser **Moms** angivelsesgrundlaget og tillader kopiering af
   rubrikker. **Hent PDF** viser **Klar til indberetning**, periode og beløb.
   En efterfølgende postering med dato i perioden afvises.
5. **Genåbn momsperiode** kræver en begrundelse og den aktuelle ret til review.
   Den oprindelige perioderække bevares; genåbningen appendes til revisionssporet
   og beslutningshistorikken. Momstallene bliver foreløbige igen. Et nyt review
   kan lukke samme periode på ny. En indberettet periode kan ikke genåbnes.
5. Når indberetningen er gennemført uden for Rentemester, åbn **Perioder og låsning**,
   vælg **Registrér indberetning** på den lukkede periode, og angiv
   **Kvitteringsreference**. Det registrerer den eksterne kvittering mod det
   oprindelige lukkegrundlag; det sender intet til myndighederne. Perioden bliver
   **Indberettet**, og momsoverblikket vælger næste aktive periode. Kvitteringen
   kan ikke ændres, perioden kan ikke genåbnes, og gentagelse med samme reference
   opretter ingen nye beslutninger eller audit-hændelser.

Kontrollerne omfatter bankafstemning, uafhængige DKK-kontrolkonti, åbne bilag,
batcher og undtagelser, udestående poster, journalhashkæde og sekvens,
journalbalancer og modposteringer, originale bilagsfiler og faktura-PDF'er,
auditlog, saldobalance, momskontrol og SQLite-integritet. Native packet-formatet
`rentemester.native-period-close.v1` (version 4) binder også journalens hoved og
auditrevisionen, så en ny balanceret postering kan gøre et review forældet.
Review, periodelås, beslutning og audit gemmes atomisk under aktuelle rettigheder.

**Periodelås** viser periodernes effektive status og kan også udføre almindelig
lukning/genåbning. API'et er `GET .../periods`, `GET .../periods/close-readiness`,
`POST .../periods/close-review`, `POST .../periods/close` og
`POST .../periods/reopen` og `POST .../periods/report`. Tidligere importer og endnu ikke porterede auditforløb,
fx bankkorrektioner og særlige debitorkrav, afvises i kontrollen. Tvangslukning
er endnu ikke porteret.

## Registrér et anlæg og bogfør afskrivning

Rust-serveren understøtter anlægskartotek og lineær afskrivning i DKK gennem det
eksisterende web-UI. Konti vælges eksplicit fra virksomhedens egen kontoplan.

1. Opret om nødvendigt anlægskonto (aktiv/debet), akkumulerede afskrivninger
   (aktiv/kredit) og afskrivningskonto (udgift/debet) under **Kontoplan**.
2. Indlæs købsbilaget under **Bilag**. Bogfør anskaffelsen på anlægskontoen via
   **Posteringer → Ny postering**, vælg bilaget i **Bilag (valgfrit)** og den
   korrekte modkonto. Denne enkle formular bogfører et samlet beløb uden
   momsfordeling; et køb med fradragsmoms skal have sin fulde, korrekte
   anskaffelsespostering, før det registreres her.
3. Åbn **Anlæg → Registrér anlæg**. Vælg de tre konti, købsbilag, navn, kategori,
   anskaffelsesdato, kostpris og levetid. Registreringen opretter planen; den
   bogfører ikke købet igen. Rust afviser kostpris, der ikke er kapitaliseret
   på bilaget og anlægskontoen, eller allerede er fordelt til andre anlæg.
4. Vælg **Beregn afskrivning** på aktivet. Dialogen viser perioden og det konkrete
   beløb. Bekræft **Bogfør afskrivning**. Afskrivningskontoen debiteres og
   akkumulerede afskrivninger krediteres med link til det originale bilag.
   Anlæggets restværdi og resultatopgørelsen opdateres.

Beregningen bruger hele øre. Eksempel: 1.000 kr. over tre perioder giver
333,33 + 333,33 + 333,34 kr. Samme periode kan ikke bogføres to gange, og
periodelåsen gælder også afskrivninger. Brugeren vælger dato, levetid og
regnskabsmæssig behandling; afskrivning kan ikke ligge før anskaffelsen.
Originalfilens integritet og aktuelle bogføringsrettigheder kontrolleres ved
registrering og afskrivning. En generel tilbageførsel af en registreret
anskaffelse eller afskrivning afvises, indtil et samlet korrektionsforløb er
porteret. Gamle modposterede afskrivninger kræver tilsvarende afklaring.

API: `GET/POST .../assets`, `GET .../assets/:id/next-depreciation` og
`POST .../assets/:id/depreciate`. Straksafskrivning, anlægskorrektioner og andre
valutaer er stadig under omskrivning; straksafskrivningsknappen vises derfor
ikke på Rust-backenden. Eventuel eksisterende straksafskrivningshistorik vises.

Browserkontrollen gennemfører bilagsindlæsning, manuel kapitalisering,
registrering og første afskrivning, verificerer restværdi og regnskabsrapporter
og gentager læsningen efter servergenstart. Den afviser et ubogført aktiv,
dobbelt disponering af kostpris og dobbelt afskrivning. Skærmbilledet gemmes
som `/tmp/rentemester-rust-assets.png`. Kør forløbet isoleret med
`python3 scripts/rust/run-web-demo.py --check-assets`; den bruger et nyt syntetisk
workspace og kontrollerer samme anlægsforløb igen efter genstart.

## Opret kunde, udsted faktura og bogfør salget

1. Under **Kontoplan** oprettes den ønskede omsætningskonto med type
   **Indtægt** og normal saldo **Kredit**, en debitorkonto med type **Aktiv**
   og normal saldo **Debet**, samt en salgsmomskonto med type **Moms** og
   normal saldo **Kredit**. Bekræft rollerne **Debitorer** og **Salgsmoms**.
   Kontonumrene vælges lokalt; produktet indsætter ingen virksomhedsbestemt kontoplan.
2. Åbn **Kontakter → Tilføj kunde**. Gem navn, adresse, CVR/moms og
   betalingsfrist. Kundekortet viser også åbne tilgodehavender på tværs af år.
3. Åbn **Fakturaer → Udsted faktura**, vælg kunden og indtast dato,
   sælgeroplysninger og linjernes beskrivelse, antal og enhedspris ekskl. moms.
   Virksomhedens navn/adresse og kundens oplysninger udfyldes fra stamdata,
   når felterne ikke er overstyret. Betalingsfristen følger kundekortet eller
   virksomhedens standard, medmindre en forfaldsdato angives eksplicit.
4. **Forhåndsvis** viser PDF uden at oprette faktura eller bruge et nummer.
   **Udsted faktura** gemmer JSON og PDF som uforanderlige originaler med hash,
   aktør og opbevaringsfrist. Nummeret følger regnskabsåret og den fælles
   fortløbende fakturaserie, eksempelvis `2026-0001`.
5. På fakturalisten vælges **Bogfør faktura**, og omsætningskontoen vælges.
   Bekræft **Bogfør salget**. Debitorkonto og salgsmoms følger de eksplicit
   bekræftede roller. Udstedelse og bogføring er separate handlinger.
6. To linjeenheder til 500 kr. giver 1.000 kr. salg, 250 kr. salgsmoms og
   1.250 kr. tilgode hos kunden. Resultatopgørelsen, momsoversigten og
   kundekortet viser disse beløb. **Hent PDF** læser altid de gemte bytes;
   ændrede stamdata eller servergenstart omskriver ikke den udstedte faktura.

Dette forløb understøtter foreløbig fulde danske DKK-fakturaer med standard
25% moms og danske CVR/momsidentiteter. Ugyldige beløb og datoer, nummergenbrug
og dobbelt bogføring afvises. PDF understøtter det nuværende latinske tegnsæt,
herunder danske bogstaver; tekst uden for tegnsættet afvises ved input.
Fremmed valuta, andre momstyper, offentlig e-faktura,
og ekstern maillevering mangler stadig i Rust.
Der sendes ingen fakturaer eksternt i dette forløb.

## Ret kundens oplysninger

Under **Kontakter** vælges **Redigér** på kunden. Ret navn, adresse,
CVR/moms, kontaktoplysninger, betalingsfrist eller noter, og vælg
**Gem ændringer**. Et tømt valgfrit tekstfelt ryddes fra kundekortet.
Fremtidige fakturaer og forhåndsvisninger bruger de nye oplysninger;
allerede udstedte fakturaers JSON og PDF bevares uændret.

API'et bruger `PATCH /api/companies/{slug}/customers/{id}`: udeladte felter
bevares, og eksplicit `null` rydder valgfrie felter. Ændringen og auditsporet
gemmes under samme transaktion og aktuelle rettighed til virksomhedsstamdata.
Ukendte felter, tomt navn, ugyldig valuta/betalingsfrist og ukendt kunde afvises.
Arkiverede kunder redigeres ikke gennem dette forløb. I workspaces med tidligere
GDPR-sletninger afvises redigering foreløbig, indtil det tilhørende identitets-
og aliasforløb er porteret; tidligere sletningsbeviser må ikke omgås ved navneskift.

## Opret leverandør og brug oplysningerne på et køb

1. Vælg **Kontakter → Tilføj leverandør**. Gem navn, adresse og kontaktoplysninger.
   Angiv land og identitetstype eksplicit: **Dansk CVR**, **EU-momsnr.** eller
   **Ikke-EU**. Dansk CVR normaliseres til `DK` efterfulgt af otte cifre.
   Land, type og identifikator skal passe sammen. Uden eksplicit land/type
   gemmes kontakten som uafklaret; et vilkårligt nummer bliver ikke et momsbevis.
2. En eventuel **Standard udgiftskonto** skal findes som aktiv udgiftskonto i
   virksomhedens egen kontoplan. Momsbehandling kan gemmes på leverandørkortet.
   Bogføringsforløbet kræver fortsat, at man vælger og kontrollerer konto og
   momsbehandling på det konkrete bilag.
3. Vælg **Bilag → Indlæs bilag → Udfyld fra leverandør (valgfri)**. Valget
   kopierer navn, adresse og den typede identitet til bilagets afsenderfelter.
   Ret eventuelle afvigelser efter det faktiske bilag, udfyld beløb og øvrige
   fakturaoplysninger, og indlæs originalfilen. Valget er en udfyldningshjælp;
   det ændrer ikke indholdet af den modtagne fil.
4. Bogfør bilaget mod en importeret udgående bankpost som i købsforløbet ovenfor.
   Momsfradraget afgøres stadig af bilagsbeviset og de gældende bogføringsregler.
5. **Kontakter → Redigér** kan nu ændre leverandøren. Tømte valgfrie felter ryddes.
   Eksisterende bilag beholder deres oprindelige afsender og originalfil.

API'et understøtter `POST /api/companies/{slug}/vendors` og delvis
`PATCH /api/companies/{slug}/vendors/{id}` under den aktuelle rettighed til
virksomhedsstamdata. Ændringen og auditsporet er atomiske. Som ved kunder afvises
vedligehold i workspaces med tidligere GDPR-sletninger foreløbig, indtil deres
identitets-/aliasforløb er porteret. Arkiverede leverandører redigeres ikke her.

## Bogfør en ubetalt leverandørfaktura og afstem betalingen senere

1. Opret en kreditorkonto under **Kontoplan**, med type **Gæld** og normal
   saldo **Kredit**, og bekræft rollen **Kreditorer**. Standardmoms kræver også
   den bekræftede købsmomskonto og virksomhedens momsregistrering.
2. Indlæs købsbilaget med originalfil, fakturadato og parter under **Bilag**.
   Købet må ikke allerede være bogført direkte mod banken.
3. Åbn **Leverandørfaktura → Registrér leverandørfaktura**. Vælg bilag,
   udgiftskonto, momsbehandling og forfaldsdato, eventuelt også leverandørkortet.
   Bilagsdatoen skal være originalens dato; den kan ikke flytte købet til en
   anden periode. Forfaldsdatoen må ikke ligge før bilagsdatoen. Registrér.
4. En faktura på 1.250 kr. med 250 kr. dansk moms bogføres som 1.000 kr.
   udgift, 250 kr. købsmoms og 1.250 kr. leverandørgæld. Den åbne kreditorliste
   viser gæld og forfald; der er endnu ingen bankbevægelse.
5. Når betalingen er gennemført, importeres den udgående bankpost. Vælg
   **Markér betalt** på kreditorposten og angiv **Banktransaktions-id**.
   Handlingen afstemmer en allerede gennemført betaling; den sender ikke penge.
   En bankbetaling på 500 kr. efterlader 750 kr. åbent. En efterfølgende betaling
   på 750 kr. udligner resten og viser fakturaen under **Betalte**.

Betalingerne berører kun bank og den oprindeligt bogførte kreditorkonto.
Ændringer af kreditorkontorollen flytter ikke gammel gæld. Udgiften og momsen
bogføres én gang. Bankpostens fulde beløb og dato bruges, og beløbet må ikke
være større end den åbne gæld. En bankpost kan ikke afstemmes to gange.
Originalfilen kontrolleres før både registrering og betaling og bevares uændret.

`GET /api/companies/{slug}/payables` understøtter `status=open|paid|overdue|all`
og `asOf=YYYY-MM-DD`. Historiske saldi medtager kun betalinger med betalingsdato på eller før
den valgte dato. Registrering og betaling bruger
henholdsvis `POST .../payables` og `POST .../payables/{id}/pay`, begge med
`confirm: true`, aktuelle bogføringsrettigheder og atomisk journal-/domæne-/auditspor.

Forløbet understøtter DKK-køb med standardmoms, momsfri behandling eller
ikke-fradragsberettiget moms. Det genbruger købskontrollen fra bankbogførte
udgifter, også for bilagets momsfordeling. Fremmed valuta og særskilt korrektion
eller kreditering af kreditorposter mangler stadig. Generel tilbageførsel af
kreditorposten eller dens betalinger afvises, så gældsregister og ledger ikke
kan komme ud af trit.

## Afstem kundens betaling mod fakturaen

1. Importér de indgående betalinger på fakturaens DKK-bankkonto under **Bank**.
   CSV'en skal have en entydig reference til hver indbetaling, eksempelvis:

   ```csv
   date;text;amount;currency;reference
   2026-10-18;Første delbetaling;500;DKK;betaling-1
   2026-10-18;Sidste delbetaling;750;DKK;betaling-2
   ```

2. Åbn den bogførte faktura på 1.250 kr. under **Fakturaer**. Vælg **Afstem**,
   angiv `betaling-1` som bankreference, og bekræft. Fakturaen har nu 750 kr.
   tilbage at modtage.
3. Afstem derefter `betaling-2`. Fakturaen vises som **Betalt**, og både
   kundekortet og den bogførte debitorkonto viser nul tilgodehavende.
4. Banken viser begge indbetalinger som afstemt. Resultat og moms ændres ikke:
   betalingen flytter beløbet fra debitorer til bank og bogfører intet nyt salg.

Afstemningen bruger fakturaens oprindeligt bogførte debitorkonto. En senere
ændring af kontorollen flytter ikke det gamle tilgodehavende. Bankkontoens
konkrete ledger-mapping er tilsvarende styrende for bankdebeten. Faktura,
bankbetaling, journalpost og betalingsregistrering forbindes i samme transaktion
med auditspor; datoen følger den importerede bankpost.

En bankpost kan kun afstemmes én gang, og hele bankpostens beløb bruges.
Delbetalinger understøttes; overbetaling, tvetydig reference, forkert beløb,
fremmed valuta, betaling før fakturadato og manglende bogføring afvises.
Restbeløbet efter en delkreditering kan betales på samme måde. Fakturaer med
tab på debitorer kræver deres endnu ikke porterede afregningsforløb. Beløb, der afviger mellem fakturaens rest og ledgerens
tilgodehavende, må ikke skjules ved at markere fakturaen betalt.

## Kreditér en faktura

1. Åbn en bogført faktura under **Fakturaer**, og vælg **Kreditér**.
2. Angiv en begrundelse og eventuelt et beløb inklusive moms. Et tomt beløbsfelt
   krediterer hele det beløb, der endnu kan krediteres: originalens bruttobeløb
   minus tidligere kreditnotaer. Betalinger reducerer ikke dette loft.
3. Bekræft **Udsted kreditnota**. Listen, kundekortet, resultatopgørelsen og
   momsoversigten opdateres fra de bogførte effekter.

Eksempel: En ubetalt faktura på 625 kr. indeholder 500 kr. salg og 125 kr.
moms. En delkredit på 250 kr. reducerer salget med 200 kr. og momsen med
50 kr. De resterende 375 kr. kan betales eller krediteres. Krediteres resten,
vises fakturaen som **Krediteret**, og dens tilgodehavende, salg og moms er nul.

Kreditnotaen får eget nummer i serien `CN-<regnskabsår>-<nummer>`, egen dato,
begrundelse og direkte reference til originalfakturaen. Originalfakturaens JSON
og PDF bevares uændret. Kreditnotaens uforanderlige JSON-original kan åbnes fra
**Bilag**; registrering, hash, opbevaringsfrist, journalpost og forbindelsen til
originalen gemmes under samme transaktion og aktørkontrol.

Bogføringen spejler originalens debitorkonto, omsætningskonti og momskoder.
Senere kontorolleændringer flytter ikke gamle beløb. Ved gentagne delkrediteringer
fordeles ørebeløbene kumulativt, så den sidste fulde kreditering udligner hver
oprindelig konto præcist. Kreditering uden begrundelse, over kreditloftet,
før originalens dato eller af en allerede fuldt krediteret faktura afvises.

En kreditering af en allerede betalt faktura kan give kunden penge til gode;
listen viser da **Overbetalt**. Brug nedenstående forløb til at afstemme
kundens tilbagebetaling. Generel tilbageførsel må ikke bruges til at omgå
fakturaens eller kreditnotaens sammenhængende korrektionsforløb.

## Afstem en tilbagebetaling

1. Kreditér den betalte faktura, så kunden har et tilgodehavende.
2. Importér kontoudtoget med den allerede gennemførte bankudbetaling.
3. Vælg **Afstem tilbagebetaling** på fakturaen, angiv udbetalingens
   **Bankreference**, og bekræft. Funktionen bogfører den importerede bevægelse;
   den sender ikke penge fra banken.

En betalt og fuldt krediteret faktura på 1.250 kr. kan fx tilbagebetales med to
bankudbetalinger på 500 og 750 kr. Efter første afstemning skylder virksomheden
kunden 750 kr. Efter den anden vises **Refunderet**, og debitorkontoen er udlignet.
Salg og moms ændres ved kreditnotaen; tilbagebetalingerne berører kun den
oprindelige debitorkonto og den importerede banks tilknyttede finanskonto.

Hver afstemning bruger hele bankpostens beløb og dato. Retningen skal være en
udbetaling i DKK, referencen skal være entydig, og beløbet må højst svare til
kundens resterende tilgodehavende. Genbrug af en bankpost afvises. Oprindelige
faktura- og kreditnotafiler bevares, og senere ændringer i debitorkontorollen
flytter ikke den oprindelige saldo. Tilbagebetaling i fremmed valuta og
særskilte tabs-/kravforløb mangler fortsat.

## Tilbagefør en postering

Åbn posteringen under **Posteringer**, vælg **Tilbagefør postering**, og angiv
dato og begrundelse. Gennemse og bekræft tilbageførslen. Originalen bevares;
der oprettes en ny postering med omvendt debet og kredit, og begge posteringer
viser deres forbindelse. En allerede tilbageført postering kan ikke tilbageføres igen.

Ved demoens postering på 125,50 kr. går begge kontosaldi tilbage til nul efter
tilbageførsel. Saldobalancens samlede debet og kredit er hver 251,00 kr., fordi
både originalen og tilbageførslen stadig indgår i historikken.

## Kontroller hele forløbet

Med Playwrights Chromium og `pdftotext` fra Poppler installeret:

```sh
python3 scripts/rust/run-web-demo.py --check
```

Kontrollen bruger en rigtig browser og den rigtige Rust-server uden mockede
API-svar. Den opretter første ejer i browseren, afviser ugyldige og allerede
brugte installationslinks, gennemfører login og MFA, bogfører 125,50 kr., kontrollerer
saldobalancen, genstarter serveren og logger ind igen med en recovery code.
Den kontrollerer derefter den bevarede postering og saldobalance samt, at
samme idempotente bogføringskald ikke opretter en dublet. Et skærmbillede gemmes
som `/tmp/rentemester-rust-first-journey.png`. Testen gennemfører også en tilbageførsel i browseren, kontrollerer originalens
uforanderlighed og saldobalancen og afviser dobbelt tilbageførsel og tilbageførsel
af en tilbageførsel. Rapporttesten åbner alle tre rapportsider, bruger de
rigtige downloadknapper og kontrollerer CSV-indhold, læsbar PDF-tekst og
deterministiske PDF-bytes. Filerne gemmes i `/tmp/rentemester-rust-reports/`.
Bilagstesten indlæser en kassebon i browseren, kontrollerer originalfilens
nøjagtige bytes før og efter genstart og afviser dublet, forkert filtype og
uautentificeret adgang. Skærmbilledet gemmes som
`/tmp/rentemester-rust-documents.png`. Derefter oprettes bankkontoen i UI'et,
en CSV importeres, bilaget matches og udgiften bogføres. Testen kontrollerer
bankafstemning og resultat, afviser manglende momsvalg, fremtidig dato og
dobbelt bogføring og verificerer resultatet efter endnu en genstart.
Skærmbilledet gemmes i `/tmp/rentemester-rust-expense.png`.
Valutatesten indlæser to EUR-bilag i browseren, bruger både udledt DKK-kurs og
importeret bankkurs, kontrollerer bogføringskvitteringernes omregning og
verificerer det samlede DKK-resultat og alle bankmatches efter genstart.
Skærmbilledet gemmes i `/tmp/rentemester-rust-fx-expense.png`.
Opsætningstesten gemmer stamdata og momsregistrering gennem UI'et, opretter
udgifts- og købsmomskonto, bekræfter rollen og bogfører en dansk faktura med
momsfradrag. Den kontrollerer nettoomkostning, købsmoms og fire bankmatches,
afviser inkompatibel rolle, kontooverskrivning og afregistrering med åben moms,
og genstarter serveren for at kontrollere, at opsætning og resultat bevares.
Skærmbilledet gemmes i `/tmp/rentemester-rust-company-setup.png`.
Momstesten åbner siden efter det danske køb, kontrollerer 25 kr. købsmoms,
−25 kr. momstilsvar, foreløbige rubrikker og en rigtig PDF-download med samme
tal, periode og frist. Den kontrollerer også efter genstart og afviser anonym
adgang. Skærmbilledet gemmes i `/tmp/rentemester-rust-vat.png`.
Salgstesten opretter konti og roller, opretter kunden i UI'et, forhåndsviser og
udsteder en faktura og bogfører salget. Den afviser nummergenbrug og dobbelt
bogføring og kontrollerer tilgodehavende, resultat og moms efter genstart samt,
at den gemte faktura-PDF har uændrede bytes. Fakturaen gemmes som
`/tmp/rentemester-rust-reports/native-sales-invoice.pdf`, og et skærmbillede
som `/tmp/rentemester-rust-sales.png`.
Betalingstesten importerer to indbetalinger, afstemmer dem gennem fakturalistens
bekræftelsesdialog og kontrollerer den delvise og fulde betaling uden gentaget
salg eller moms. Efter genstart kontrolleres de bevarede betalingsposteringer
sammen med de efterfølgende kreditnotaer og tilbagebetalinger. Skærmbilledet gemmes som
`/tmp/rentemester-rust-settlement.png`.
Kreditnotatesten udsteder og bogfører en anden faktura i UI'et, krediterer den
først delvist og derefter fuldt, og kontrollerer ændringerne i tilgodehavende,
salg og moms. Den afviser manglende begrundelse, overkreditering og gentagen
fuld kredit. Efter genstart kontrolleres kreditstatus, begge JSON-originalers
nøjagtige bytes og originalfakturaens uændrede PDF. Skærmbilledet gemmes i
`/tmp/rentemester-rust-credit-note.png`.
Tilbagebetalingstesten krediterer den allerede betalte første faktura,
importerer to udbetalinger og afstemmer først 500 og derefter 750 kr. gennem
UI'et. Den kontrollerer status **Refunderet**, nul debitorsaldo, otte afstemte
bankposter og uændrede originalfiler efter genstart. Dobbelt afstemning afvises.
Skærmbilledet gemmes i `/tmp/rentemester-rust-refund.png`.
Kunderedigeringstesten omdøber kunden gennem UI'et, ændrer adresse og
betalingsfrist og rydder telefonnummeret. Den kontrollerer delvis PATCH,
afviste ugyldige felter, nye forhåndsvisningers adresse/forfald og uændrede
udstedte JSON-/PDF-originaler før og efter genstart. Skærmbilledet gemmes i
`/tmp/rentemester-rust-customer-edit.png`.
Leverandørtesten opretter et dansk leverandørkort gennem UI'et, vælger det ved
indlæsningen af købsfakturaen og bogfører/afstemmer købet. Den retter derefter
navn og adresse, rydder telefon og noter og afviser modstridende identitet og
ugyldig standardkonto. Efter genstart kontrolleres både stamdata, bilagets gamle
afsender og de nøjagtige originalbytes. Skærmbilledet gemmes i
`/tmp/rentemester-rust-vendor.png`.
Kreditortesten indlæser endnu en leverandørfaktura, registrerer den som ubetalt
og kontrollerer den første gæld på 1.250 kr. Derefter afstemmes 500 og 750 kr.
gennem UI'et. Testen kontrollerer **Betalt**, nul kreditorsaldo, ti afstemte
bankposter og ét samlet udgifts-/momsbeløb uden gentagelse ved betaling. En
sidste servergenstart bekræfter gældsregister, rapporter og de uændrede
originalbytes. Skærmbilledet gemmes i `/tmp/rentemester-rust-payable.png`.
Bankens saldotest importerer en kassebonbetaling med en eksplicit saldo fra
kontoudtoget. UI'et viser først differencen, derefter nul difference efter
bogføring og afstemning. Senere importer uden saldo må ikke få en opdigtet
slutsaldo. Rust-testen kontrollerer omvendt filrækkefølge, flere konti,
dubletkonflikt og uafklaret saldogrundlag; UI-testen kontrollerer forklaringen.
Skærmbilledet gemmes i `/tmp/rentemester-rust-bank-balance.png`.
Periodetesten indlæser og bogfører et køb i den foregående afsluttede momsperiode.
Den kontrollerer først, at bankposten blokerer lukningen, og gennemfører derefter
review/lukning, afvist efterpostering, begrundet genåbning og ny lukning af samme
periode gennem UI'et. Efter en ekstra servergenstart kontrolleres periodelås,
angivelsesgrundlag, PDF og afvist postering igen. Rust-testen viser også, at en
ny balanceret postering gør et tidligere review forældet. Skærmbilledet er
`/tmp/rentemester-rust-closed-vat.png`; PDF'en er
`/tmp/rentemester-rust-reports/native-closed-vat.pdf`.
Testdata fjernes fra Docker efter kørslen.

## Aktuel afgrænsning

Launcheren opretter workspacets filer lokalt. Første ejer oprettes i browseren
via et privat installationslink, som den betroede lokale `setupActor` aktiverer
fra serverkonfigurationen. Linket gælder en time og lukkes efter vellykket
opsætning. Servergenstart kontrollerer den gemte status og genåbner ikke en
afsluttet opsætning. Demoens HTTP-loopback leverer bekræftelseslinks som
private lokale filer. HTTPS-servere bruger den konfigurerede mailgateway
beskrevet under **Login og gendannelse på en HTTPS-server**.

Standardprofilen bruger den beskyttede Linux-profil; demoen kan derfor køres
i Docker på macOS. Native lokal drift på Mac har en særskilt profil, beskrevet
nedenfor. Serverens `--config` bruges med native
workspaces og en betroet lokal konfiguration; `--create` opretter et nyt workspace,
og uden flaget genåbnes det eksisterende. `--origin` angiver browserens lokale
origin, når serveren kører bag Docker-portmapping.

Rust-cockpittets startside viser foreløbig virksomhedsvælgeren. Resten af
porteføljeoverblikket, øvrige bogførings-, rapport- og eksportforløb, desktop og produktionsdrift
er fortsat under omskrivning. Denne demo er ikke den færdige Rust-migration.

Browserkontrollen registrerer også en ekstern indberetningskvittering via UI’et,
kontrollerer næste aktive momsperiode og afviser ændret reference, genåbning og
ny bogføring i den indberettede periode, også efter servergenstart. Skærmbilledet
gemmes som `/tmp/rentemester-rust-period-report.png`.

## Budgetter, periodisering og kørsel i Rust

De eksisterende sider **Budget**, **Periodisering** og **Kørsel** bruger nu
Rust-serveren direkte.

- **Budget:** Gem et beløb på en konto og måned, ret beløbet ved at gemme en ny
  revision, og sammenlign med faktisk bogføring. Seneste revision gælder;
  historikken bevares. Sammenligningen følger kontoens indtægts-/udgiftstype.
  Dimensionsbudgetter og dimensionsfordeling er endnu ikke porteret; siden
  viser derfor eksplicit, at budgettet er på kontoniveau.
- **Periodisering:** Opret en forudbetalt eller skyldig omkostning eller udskudt
  omsætning med bilag, eksplicitte konti, datoer og antal perioder. Gennemgå
  planen og bekræft de enkelte posteringer. En månedsplan forankret den 31. januar
  bruger den 28. februar og den 31. marts. Sidste periode bærer afrundingsresten.
  Der er ingen automatisk momsbehandling. Generelle tilbageførsler af planens
  posteringer afvises, indtil et samlet korrektionsforløb er porteret.
- **Kørsel:** Registrér tur, formål, kilometer, køretøj/fører og den valgte sats
  med satsgrundlag. Registeret viser regnskabsårets og månedernes summer og
  bevarer de originale ture og revisionsspor. Dette er dokumentation;
  registreringen bogfører ikke noget i journalen.

En samlet syntetisk browserkontrol kører oprettelse, ændring/postering,
rapportkontrol og læsning efter servergenstart:

```sh
python3 scripts/rust/run-web-demo.py --check-feature parallel
```

Enkeltforløb kan køres med `--check-feature budget`, `--check-feature accruals`
eller `--check-feature mileage`. Alle bruger nye syntetiske workspaces.

## Fælles Rust-klient, CLI og MCP

`rentemester-client` giver CLI og MCP én fælles HTTP-forbindelse med eksplicit
serveradresse. HTTPS kræves, bortset fra udtrykkeligt tilladt HTTP på en literal
loopbackadresse. Klienten følger ikke redirects, gentager ikke writes automatisk
og skifter ikke til lokale data ved forbindelsesfejl. En ukendt write-status
returneres som ukendt; den oprindelige idempotency-nøgle bevares.

Den første forbindelsesvej bruger en eksisterende MFA-godkendt sessionscookie i
en privat fil med filtilstand 0600. Filen indeholder Cookie-headerens værdi;
hemmeligheden må ikke indsættes i kommandolinjen eller logs. Denne vej er et
udviklings-/operatørinterface. CLI har også password/MFA-login som beskrevet
nedenfor; et fuldt profilsystem er endnu ikke færdigt.

```sh
cargo build -p rentemester-cli -p rentemester-mcp

target/debug/rentemester-rust remote --server https://SERVER \
  --cookie-file /private/session.cookie trial-balance --company VIRKSOMHED

target/debug/rentemester-mcp --server https://SERVER \
  --cookie-file /private/session.cookie
```

CLI understøtter health, bruger/virksomheder, journal, saldobalance,
resultatopgørelse og balance samt eksplicitte posteringer/tilbageførsler med
serverens JSON-envelope på stdin. MCP tilbyder tilsvarende tools over
stdio med MCP 2025-11-25. De genbruger serverens bogføring og rettighedskontrol;
fuld CLI/MCP-funktionsparitet er stadig under omskrivning.

Den samlede browserkontrol kan også kontrollere de native CLI/MCP-processer mod
samme MFA-session og server:

```sh
RM_NATIVE_CLI_BIN="$PWD/target/debug/rentemester-rust" \
RM_NATIVE_MCP_BIN="$PWD/target/debug/rentemester-mcp" \
python3 scripts/rust/run-web-demo.py --check-feature parallel
```

Kontrollen sammenligner rapporterne med browserens API-svar og gentager en
allerede bogført postering via MCP med samme nøgle uden at oprette en dublet.


## Tilbagevendende fakturaer og Overblik

**Fakturaskabeloner** kan oprettes med uge-, måneds-, kvartals- eller årsinterval.
En bruger gennemgår og bekræfter hver udstedelse. Genereringen bruger samme
fakturanummerering, beregning og originale JSON/PDF som almindelige fakturaer.
Gentagen generering af samme skabelonperiode giver samme dokument. Skabeloner
kan pensioneres; udstedelsernes kundesnapshot og originaler bevares.
Der er endnu ingen automatisk scheduler eller afsendelse; den porterede
udstedelsesvej understøtter DKK og dansk 25 % moms. Bogføring sker særskilt
via fakturaforløbet. CVR-nummer kan redigeres under virksomhedens stamdata.

Virksomheden åbner nu på **Overblik** med resultat, månedsdiagram, bank,
moms, debitorer, kreditorer, bilag og seneste posteringer fra de fælles
Rust-læsere. Debitorer viser aktuelle åbne fakturaer på tværs af levende
regnskabsår; historiske debitorsaldi pr. dato er endnu ikke tilgængelige.
Opgaver kan markeres som gennemgået med en valgfri note. Handlingen bevarer
de oprindelige fakta, registrerer bruger og tidspunkt og bogfører ikke noget.
Gentagen markering er en no-op. Revisoreksport er også tilgængelig.
Den samlede `--check-feature parallel` inkluderer begge sider og genstart.

## CLI-login med adgangskode og MFA

```sh
target/debug/rentemester-rust remote --server https://SERVER login \
  --email BRUGER --factor totp --session-out /private/ny-session.cookie
```

Processen læser adgangskode og derefter engangskode som to linjer fra privat
stdin. Den afviser terminalinput med ekko; indsæt aldrig hemmeligheder som
kommandoflag. `--factor backup` bruger en recovery code. Filen oprettes som
0600 på Unix, og en eksisterende fil overskrives aldrig. MFA-tilmelding foregår
fortsat i browseren. Den udstedte session bruges derefter med `--cookie-file`.

## Native GPUI-desktop

Craten `rentemester-desktop` bruger Longbridges `gpui-kit` 0.7.3 og det fælles
HTTP-klientbibliotek. En faktisk macOS-app har vist serverens virksomhed og
saldobalance i et native GPUI/Metal-vindue. Adgangskode og TOTP-MFA er også
verificeret gennem det native vindue uden cookie-fil. Appen kan både forbinde
til en ekstern server og administrere en lokal Rust-server. Manuel bogføring kan gennemgås og bekræftes i det native vindue; flere
skriveforløb er under udvikling.

Desktop kræver Rust mindst 1.95. Server/CLI/MCP kan fortsat bygges med projektets
fastlåste 1.94.1, og desktop er udeladt fra standardmedlemmerne.

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo +stable build --locked -p rentemester-desktop -j4
crates/rentemester-desktop/build-macos.sh
```

Den installerede stable 1.99.1 er brugt til native verifikation. En eksplicit
serverorigin og særskilt tilladelse til HTTP på loopback kræves. Der er ingen
automatisk overgang til lokale data ved netværksfejl.

Vælg lokal drift i vinduet, vælg datamappen og brug **Opret og start lokalt**
ved første oprettelse. Standardmappen er `~/Library/Application Support/Rentemester`.
Apppakken indeholder Rust-serveren og web-UI’et. Første ejer og MFA-tilmelding
oprettes i browseren; appen kan læse det private bekræftelseslink og derefter
logge ind med adgangskode og TOTP eller backupkode. Serveren bindes til en fri
loopback-port. Stop og genåbn bruger de samme data og nøgler.

Det samlede lokale forløb er afprøvet med syntetiske data: native oprettelse,
browseropsætning, native login, saldobalance, stop og genåbning med uændret
konfiguration og nøgler. CLI-valgene er `--local`, `--create-local` og
`--local-workspace PATH`. Ved udvikling kan `--server-binary PATH` og
`--web-dir PATH` angive byggede filer. macOS-pakken har appikonet fra
`assets/brand/rentemester-app-icon.svg`, eksporteret til PNG og ICNS.

## Native lokal server på Mac

En privat serverkonfiguration kan vælge `"runtimeProfile":"local-user-macos"`.
Standardværdien er fortsat `dedicated-linux`. Mac-profilen kræver en almindelig
bruger med samme reelle, effektive og gemte bruger-/gruppeidentitet, umask 0077,
APFS og privat workspace. Filer kontrolleres for ejer, links, identitet og ACL.
En ikke-nedarvet ACL, der alene nægter sletning, tillades på forældremapper, så
normal Application Support kan bruges; selve workspacet skal være ACL-frit.

Profilen stoler på kode under den lokale brugerkonto. Linux-profilens separate
broker, capabilities og no-new-privileges har deres eksisterende kontrakt.
Mac-serveren kræver loopback-binding og lokal origin. En integreret test opretter
et syntetisk workspace i Application Support, afviser for åben umask og en ekstra
samtidig ejer, stopper processen og genåbner de samme filer:

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p rentemester-server --test macos_local
```

## Kontakter og fakturering gennem Rust CLI/MCP

Native remote-kommandoer og MCP-tools kan læse kontakter, konti, bilag og
fakturaer, oprette/rette kunder og leverandører samt forhåndsvise, udstede og
bogføre fakturaer. Aktøren kommer fra den godkendte serversession.

For dette nye `remote`-interface er `--confirm` et selvstændigt flag; udstedelse
kræver også `--preview-reviewed`. Dette er adskilt fra TypeScript-CLI'ens
`--confirm yes`-syntaks. Payload på stdin er serverens nøjagtige JSON-body.
MCP kræver de tilsvarende eksplicitte bekræftelser. Forhåndsvisningen er en PDF
repræsenteret som bytes i JSON-output og udsteder ingen faktura.

Domæne-endpoints bruger endnu ikke generelle idempotency-nøgler. Klienten
opfinder ingen nøgle og gentager ingen writes. Ved ukendt udfald skal klienten
læse den aktuelle tilstand og afklare resultatet før en ny mutation.

Browserkontrollen opretter en kunde med CLI og en leverandør med MCP, retter
begge på tværs af klienterne, forhåndsviser via CLI, udsteder via MCP og bogfører
via CLI. Original, beløb og stamdata kontrolleres igen efter genstart.

## Revisoreksport

Fra **Overblik → Revisor-eksport** vælges en periode og download bekræftes.
Rust-serveren håndhæver aktuel `company.export`-adgang og samler JSON/CSV,
rapporter, revisionshistorik og kontrollerede originaler i en portabel tar-fil.
Manifestet indeholder filernes størrelse og SHA-256. Manglende eller ændrede
registrerede originaler afviser hele eksporten. Eksporten skriver et auditspor
og ændrer ingen bogføring.

Native release-/regelsætprovenance er endnu ikke komplet. Manifestets
`provenance` er derfor null med en eksplicit begrænsning i manifest og README.
Pakken er en overdragelse til revisor; den er ikke en verificeret backup.
Browserkontrollen downloader den faktiske pakke, læser manifest og indhold,
sammenligner originalerne med deres downloads og gentager efter genstart.


## Slet ubrugte kontakter

Fra **Kontakter** kan en kunde eller leverandør slettes efter bekræftelse.
En åben eller overbetalt kundefaktura blokerer handlingen, også efter kunden
har skiftet navn. Leverandørgæld og historiske fremmednøgler bevares og kan
blokere sletning. Handlingen skriver auditspor; fakturasnapshots, bilag og
posteringer ændres ikke. Den rigtige webdialog, manglende bekræftelse, åben
faktura og persistens efter genstart er kontrolleret med syntetiske data.
Dette er kontaktvedligeholdelse; GDPR-anonymisering er et særskilt forløb.

## Dimensioner og klassifikation

**Dimensioner** kan oprette dimensioner og medlemmer. Fra en postering kan
en linje fordeles efter en hash-bundet forhåndsvisning og eksplicit bekræftelse.
En reviewer kan erstatte en fordeling med begrundelse; tidligere beslutninger
og bruger/tidspunkt forbliver synlige. Forældet review afvises. En allerede
brugt planhash kan ikke genbruges; afvisningen efterlader den gældende fordeling
intakt. Klassifikation ændrer ingen journalbeløb. Oprettelse, fordeling,
begrundet erstatning og historik er kontrolleret gennem web-UI’et og efter genstart.

## Manuel bogføring i desktopappen

Vælg virksomheden og **Ny postering**. Angiv dato, tekst, debet-/kreditkonto,
beløb og eventuelt bilag; gennemgå de nøjagtige linjer før **Bogfør postering**.
Appen gemmer den præcise anmodning og idempotency-nøgle privat før afsendelsen.
En ukendt status giver kun eksplicit gentagelse med samme anmodning og nøgle.
En ændret lokal port kræver særskilt bekræftelse og kontrol af samme lokale
profil, konfiguration, ledgeridentitet, bruger og virksomhed. Eksterne servere
forbliver bundet til den oprindelige origin.

En rigtig native postering på 12,34 med kvittering og journalaflæsning er
kontrolleret, efterfulgt af stop/genåbning og korrekt, balanceret saldobalance.
Dette forsøg brugte aktiv/egenkapital uden bilag. Genopretning efter mistet
svar er afprøvet med en faktisk TCP-fejl og samme byte-anmodning; lokal
portgenbinding har fokuserede persistens-/mismatchtests, men er endnu ikke
separat afprøvet gennem det native vindue.

## Leverandørforpligtelser og likviditetsplan

På **Likviditet** vælges prognosens startdato for hele virksomheden. Opret en
forpligtelse med eksisterende leverandør, originalt kildebilag, beløb og
betalingsrytme. Gennemgå planen før godkendelse. En godkendt forekomst kan
matches med et bilag, en kreditorpost eller en bankudbetaling. Pause og ophør
kræver begrundelse og bevarer historikken. Planlægning og match bogfører ikke.

De 13 uger viser bogført bank, åbne kunde-/leverandørposter og forventede
forpligtelser. Manglende bankgrundlag vises som ukendt; beløb i andre valutaer
omregnes ikke. Budgetbeløb uden betalingsdato bliver ikke opfundne betalinger.
Prognosen oplyser manglende dækning og er fortsat under udbygning.

CLI tilbyder `supplier-commitments`, `supplier-commitment-matches`,
`plan-supplier-commitment`, `approve-supplier-commitment`,
`change-supplier-commitment` og `match-supplier-commitment`; MCP har tilsvarende
navne med understregning. Planen er en læsning; godkendelse og ændring bruger
serverens aktuelle hash og udtrykkelig bekræftelse. Klienterne gentager ikke
automatisk en anmodning med ukendt resultat.

## Indsigt, anonymisering og opbevaring

**GDPR** kan finde en person eller kontakt med navn og identifikator og vise
indsigtsrapport, datakilder og opbevaringsgrundlag. Anonymisering kræver særskilt
bekræftelse. Serverens dato afgør opbevaringskontrollen; klienten kan ikke flytte
kontroldatoen. Beskyttede bilag og bogføringsdata bevares.

Anonymisering bruger maskerede læsninger og uforanderlige registreringer.
En anonymiseret identitet kan ikke genindføres gennem almindelig
kontaktvedligeholdelse; andre kontakter kan fortsat vedligeholdes. Dækningen
er udtrykkeligt delvis. Rapporten angiver de understøttede kilder og må ikke
læses som en garanti for fysisk sletning af alle felter, originaler og backups.

## Betalinger og original faktura gennem agentklienterne

CLI's `settle-invoice`, `credit-invoice` og `refund-invoice` samt MCP's
`settle_invoice`, `credit_invoice` og `refund_invoice` bruger serverens
eksisterende bekræftede betalings-, kreditnota- og tilbagebetalingsforløb.
De tager det præcise native JSON-input og tilføjer ingen idempotency-nøgle.
Ved ukendt resultat kontrolleres faktura, bank og journal før næste handling.

`invoice-pdf --company SLUG --id ID` og MCP's `invoice_pdf` læser den udstedte
original med gældende bilagsadgang. Svaret indeholder PDF'ens nøjagtige bytes
som JSON-heltal; læsningen opretter ingen lokale filer.

## Flere native desktopforløb

Desktopappen har nu fakturaforhåndsvisning, særskilt udstedelse og bogføring
samt resultatopgørelse, balance, saldobalance og journal med regnskabsår.
Build og fokuserede tests er bestået. Faktura- og rapportforløbene mangler
fortsat accept gennem det rigtige native vindue; Mac'en var låst ved forsøget.
Det ændrer ikke de tidligere verificerede native login-, lokalserver- og
manuelle bogføringsforløb. Banklæsning, kontooprettelse og CSV-import med
udtrykkelig gennemgang er også integreret og består native build og tests.
Et ukendt importsvar bevares privat til manuel afklaring uden automatisk
gentagelse. Bankforløbets test i det rigtige vindue afventer også oplåsning.

## Årsrapport og skyldige beløb

**Årsrapport** forbereder regnskabstal og tydeligt markerede skabeloner fra et
valgt, lukket år. CVR og balancerede bøger skal være på plads; en genåbnet
periode blokerer igen. Sammenligningstal bruger de samme Rust-beregninger som
regnskabsrapporterne. Ejer eller revisor skal gennemgå skabelonerne og sørge
for indberetning. Funktionen er ikke en indsendelse eller et færdigt
myndighedsgodkendt dokument.

CLI læser samme grundlag med `annual-report --company SLUG
--fiscal-year-start YYYY-MM-DD --fiscal-year-end YYYY-MM-DD`; MCP bruger
`annual_report` med `company`, `fiscalYearStart` og `fiscalYearEnd`.
Manglende forudsætninger returneres uændret i rapporten. De nye læsninger
består kontrakttests. Den samlede kontrol på `3acc4aaa` har også verificeret
hele HTTP/CLI/MCP-svaret mod browserens rapport, både før og efter genstart.

**Forpligtelser** viser kreditbalancer på gældskonti og særskilt momsgrundlag,
så moms ikke tælles dobbelt. Manglende oplysninger om momsbetalinger og
fristgrundlag vises som begrænsninger. Årsrapportens indberetningsrække har
intet gældsbeløb; fristen står ukendt, når rapporteringsklasse og
indberetningspligt ikke er fastlagt. Siden giver adgang til momsdetaljer,
kopiering af beløb og eksterne indberetningssider uden at registrere betaling
eller indsende noget. Begge sider er kontrolleret gennem den rigtige browser
og efter servergenstart med syntetiske data, sammen med de tidligere
bogføringsforløb.


## Bilag gennem Rust-klienterne og desktop

CLI har `ingest-document --confirm` og `document-original`; MCP har
`ingest_document` og `document_original`. Indlæsning videresender det præcist
gennemgåede JSON én gang. Serveren validerer original, metadata, dubletter og
opbevaring. Originalfilen må højst være 25 MiB; JSON-upload må højst være
35 MiB. MCP tillader kun den større inputramme ved `ingest_document`; andre
værktøjer beholder deres grænse på 1 MiB. Original-læsningen returnerer
filtype og præcise bytes. Agentværktøjerne er bygget og kontrakttestet. Indlæsning, afvisninger og
præcise originalbytes har bestået både den første samlede browserfase og
genstartskontrollen gennem den rigtige Rust-server (Group18).

Desktop har bilagsliste, valg af original, manuelt angivne bilagsfelter,
gennemgang og bekræftet upload. Filens SHA-256 sammenholdes med serverens
kanoniske bilag. Download kontrollerer filtype og hash og gemmer en privat
fil; PDF, billeder og tekst kan åbnes i en almindelig fremviser. JSON/XML
kan gemmes. Et uafklaret uploadsvar kræver manuel afklaring før en ny upload.
Ved integration bestod 49 native tests og build; det rigtige native
vindue afventer fortsat oplåsning af Mac'en.

Fra et gennemgået og åbnet DKK-købsbilag kan desktop nu vælge en faktisk
udgående banklinje, omkostningskonto og udtrykkelig momsbehandling. Serveren
validerer og bogfører de præcist bekræftede valg; appen genlæser bilag,
bankafstemning, journal og regnskabstal. Den native levering bestod 55 tests
og build. Ved integration blev en manglende afkrydsning til originalgennemgang
rettet; faktisk vinduesaccept udestår. Valutakøb er endnu ikke omfattet.

Desktop har desuden et særskilt forløb fra gennemgået original til ubetalt
leverandørfaktura og senere betaling med en valgt banklinje. Registrering og
betaling kræver hver sin bekræftelse og kanoniske genlæsning. Den integrerede
levering bestod 62 tests og native build; faktisk vinduesaccept afventer
fortsat oplåsning.

## Knyt en banklinje til en eksisterende postering

Bankvisningen kan gennemgå serverens kandidater og knytte en importeret
linje til en allerede bogført postering med begrundelse og bekræftelse.
Serveren kontrollerer den arkiverede CSV-kilde, beløb, postering og det
aktuelle bevis igen. Handlingen gemmer afstemningslink og auditspor uden
nye journalposteringer eller ændringer af leverandørgæld. Forældede beviser
og gentagelser afvises. CLI/MCP har de samme kandidatlæsninger og bekræftede
links. Webforløbet og faktisk CLI/MCP-paritet har bestået den første samlede
fase og efter genstart (Group20).

**Ret** gennemgår et forkert link, efter at den oprindelige postering er
tilbageført og en korrekt erstatning er bogført særskilt. Serveren kræver
den faktiske tilbageførselsrelation og kontrollerer kilde, beløb og historik.
Bekræftelse med begrundelse gemmer en ny afstemningshændelse og en varig
kvittering; den foretager ikke selv tilbageførsel eller ny bogføring.
CLI/MCP tilbyder samme plan og bekræftelse med den oprindelige nøgle.
Den samlede første serverfase (Group18/19) har verificeret web og CLI/MCP,
herunder kvitteringer og afvisninger. Rettelser og de oprindelige varige kvitteringer har også bestået
genstartskontrollen (Group20).


## Tilbagevendende fakturaer gennem agentklienterne

CLI har `recurring-invoices`, `create-recurring-invoice`,
`generate-recurring-invoice` og `retire-recurring-invoice`; MCP har de samme
navne med underscore. Oprettelse, generering og afslutning kræver præcist
JSON og udtrykkelig bekræftelse. Generering udsteder en faktura med original
PDF; bogføring foretages særskilt. Samme skabelon og periode kan udtrykkeligt
hente den allerede udstedte faktura. En afsluttet skabelon afviser generering.
Den samlede serverkontrol har verificeret CLI/MCP, PDF, afvisninger og
persistens efter genstart (Group18).

## Når sessionskontrollen ikke svarer

En midlertidig fejl under kontrol af session eller adgang blokerer visning af
regnskaber og giver **Prøv igen**. Den bliver ikke behandlet som en bekræftet
udlogning. En faktisk udløbet eller afvist session kræver fortsat login.
Serverens læsebudget for almindelig navigation fornyes efter minutvinduet;
grænserne for adgangskode og MFA er uændrede.


## Invitationer og brugeradgang

**Brugere** kan invitere en person til en valgt virksomhed, vise invitationens
status og annullere den. Modtageren accepterer invitationen uden at få en
session, bekræfter sin e-mail og opsætter MFA før almindeligt login. Ejerens
adgangsstyring kan ændre roller og tilbagekalde eller gentildele adgang;
den sidste aktive ejer er beskyttet. Invitationslinks vises ikke i lister
eller audit. Den lokale server leverer linket i sin private outbox, ligesom
bekræftelseslinks. HTTPS-serveren bruger i stedet den konfigurerede
mailgateway til invitation og efterfølgende verificering. De lokale kontroller
sender ingen mail til virkelige modtagere.

Det integrerede forløb er verificeret på den native Mac-server med to normale
brugere, herunder afvisning, tilbagekaldelse, gentildeling, ugyldige eller
brugte invitationer og login med MFA efter servergenstart. Testen oprettede
ikke brugere eller rettigheder direkte i databasen.

## Anlæg og periodisering gennem agentklienterne

CLI/MCP kan registrere et anlæg fra det faktiske oprindelige købsgrundlag,
hente næste afskrivning og bekræfte en udtrykkeligt valgt afskrivningsperiode.
Serveren beregner beløb og bogfører. Den samlede serverkontrol har
verificeret forløbet, afvisninger og genstart mod den kanoniske plan og journal
(Group18).

Periodisering har tilsvarende registrering, serverberegnet plan og bekræftet
indregning af en valgt periode. Registrering kan selv bogføre en postering;
begge skrivninger kræver udtrykkelig bekræftelse. Klienterne beregner ikke
fordeling og starter ikke indregning automatisk. Registrering, indregning,
afvisninger og genstart har bestået den samlede serverkontrol (Group18).

## Budget, kørsel og databeskyttelse gennem agentklienterne

CLI/MCP har budgetlæsning, sammenligning og udtrykkelige budgetrevisioner samt
kørselsrapport og bekræftet registrering med brugerens valgte sats og grundlag.
Serveren beregner beløb; kørselsregistrering opretter ingen journalpostering.
Databeskyttelse omfatter opbevaringsrapport, auditeret personudtræk og bekræftet
anonymiseringslag med samme begrænsede dækning som webforløbet. Originaler,
journaler og eksisterende opbevaringskrav bevares.

Alle disse forløb har bestået mod den rigtige server både før og efter
genstart (Group20). Kørselskontrollens gemte totaler omfatter alle
eksempelrejser. Den samlede kontrol omfatter 68 beståede checks og otte
yderligere browserbekræftelser; den separate virksomhedsopsætning er
kvalificeret med sine egne klientartefakter og sit eget workspace.

## Virksomhedsopsætning gennem agentklienterne

CLI har `company-profile`, `update-company-profile`, `create-account` og
`confirm-account-role`; MCP har de samme navne med underscore. Den eksisterende
`accounts` viser den fulde kontoplan og aktuelle rolleopløsninger. Skrivninger
kræver udtrykkelig bekræftelse og præcist native JSON. Klienterne vælger ingen
bank- eller momskonti automatisk og ændrer ingen journaler. Serveren kontrollerer
kontotype, normalbalance og øvrige aktuelle krav.

Den særskilte acceptkørsel bruger en ny tom syntetisk virksomhed med normal
e-mailbekræftelse og MFA. Den må ikke sammenblandes med andre forløb, fordi
kontoroller ændres. Build, 164 kontrakttests og faktisk native kontrol med normal MFA og
servergenstart er bestået. Profil, kontoplan og eksplicitte roller stemmer
fuldt mellem HTTP, CLI og MCP; journalen er uændret. Inaktive konti kan endnu
ikke oprettes gennem den eksisterende HTTP-kontrakt og er ikke dækket af
dette acceptkorpus.

## Kørsel og virksomhedsopsætning i desktop

Desktop har nu **Kørsel** med eksplicit regnskabsår, brugerens valgte tur,
sats og satsgrundlag, særskilt gennemgang og bekræftelse. Appen viser serverens
kvittering, advarsler og faktiske rapport; den beregner ingen kørselsgodtgørelse
lokalt og bogfører ikke en journalpostering.

**Virksomhedsopsætning** viser den aktuelle profil, kontoplan og serverens
manglende eller tvetydige kontoroller. Profilændring, kontooprettelse og
bekræftelse af en selvvalgt kontorolle er særskilte, gennemgåede handlinger.

Uafklarede svar blokerer nye skrivninger, indtil den oprindelige handling er
afklaret manuelt mod serveren. Den private status indeholder ingen personlige
formularfelter og genafsender intet. Den lokale servers port må ændre sig ved
genstart, mens den samme lokale profil, bruger og ledger fortsat skal stemme.

Integrationen bestod 78 native tests og build. Faktiske native vinduesforløb
er endnu ikke verificeret: Mac'en er nu låst op; den aktuelle faktiske desktopaccept er i gang.

## Flere virksomheder på Rust-serveren

**Tilføj virksomhed** opretter nu et faktisk, separat regnskab med egne
posteringer, kontoplan og uforanderligt regnskabs-ID. Den aktuelle ejer vælger
navn, kalender og momsopsætning; der kopieres ingen kontoplan eller saldi.
Virksomhedsskift og den efterfølgende handling bruger samme serverlås.
Adgang kontrolleres for den valgte virksomhed ved hver handling.

CLI har `create-company --confirm` med præcist JSON på stdin; MCP har
`create_company` med `body` og `confirm:true`. Handlingen er workspace-baseret
og har ingen `company`-vælger eller indre `confirm`. Uafklaret oprettelse skal
afklares gennem virksomhedslisten før en ny handling; den gentages ikke
automatisk. Web, normalt ejer-/læserlogin med MFA, adskilte posteringer,
rettighedsafvisninger og genstart er verificeret på den native Mac-server.
CLI/MCP har desuden verificeret to nye virksomheder og fuld kanonisk paritet
efter genstart.

## SAF-T v3-eksport

Revisor-eksportkortet har **SAF-T v3 — generér og download**. Vælg periode og
hent en TAR med XML, README og manifest. CLI har `saft-export --company SLUG
--confirm`; MCP har `saft_export`. Begge kræver det præcise native JSON med
`periodStart`, `periodEnd` og `confirm:true`. Serveren sætter eksporttidspunktet
og registrerer eksporten i auditsporet.

Dette er Rentemesters eksisterende, afgrænsede v3-profil. Manifestet medtager
regnskabs- og kildekontekst; originalbilag kontrolleres, men pakkes ikke med.
Eksporten er ikke en officiel generisk SAF-T-validering eller indberetning.
Faktisk download, klientparitet, kildekontrol og genstart er verificeret i den
samlede kørsel. En efterfølgende regression fandt, at slettede kontaktfelter
kunne genvises i eksportens stamdata. Eksporten bruger nu samme GDPR-maskering
som kontaktvisningen; rå historik og journal ændres ikke. Målrettede tests
består. Særskilt web-/CLI-/MCP-kontrol efter sletning og genstart med nyt
MFA-login består også; den dækker de felter, som den fælles kontaktmaskering
faktisk understøtter.

## Hent signeret workspace-backup fra web

Rust-UI’ets **Integritet** har nu et kort til eksplicit bekræftet download af
hele workspacets backup. Det bruger serverens signerede fil direkte og
medtager ingen serverstier eller nøgler i forespørgslen. En afbrudt forespørgsel
gentages ikke automatisk. Downloadkvitteringen beviser ikke gendannelse.

Web-handlingen og backup-/gendannelsesserveren er integreret. En isoleret
forløbstest har hentet filen fra web, fjernet kilde og privat signeringsnøgle,
verificeret med en separat offentlig nøgle og gendannet til en ny mappe. En
ny ejer logger ind med almindelig verificering og MFA; originalbilag, begge
regnskaber og genstart er kontrolleret. Importerede handlingsnøgler kan ikke
bruges til at gentage gamle posteringer under de nye legitimationsoplysninger.
Forløbet består også på den samlede hovedbranch med de integrerede
rykker- og SAF-T-funktioner.

Serverens operator konfigurerer `backupSigningKeyFile` uden for workspace og
arkiv. `rentemester-server --verify-backup ARKIV --trusted-backup-key NØGLE`
verificerer filen med en separat betroet offentlig nøgle. Gendannelse kræver
serverkonfiguration med en ny første ejer og `restoreActor`, samt
`--restore-backup ARKIV --trusted-backup-key NØGLE --confirm-target NY_ROD
--source-unavailable`. Målet skal være en ny mappe; eksisterende mapper
overtages ikke.

Arkiverede virksomheder og ikke-understøttede kontrolregistre med indhold
afvises eksplicit. Dette beviser ikke generel backup-compliance, global
engangsbrug af arkivet eller afsluttet overdragelse mellem to aktive servere.

Backup kan også hentes med `remote workspace-backup --confirm` eller MCP
`workspace_backup` med `confirm:true`. Begge bruger workspace-niveauet og
accepterer ingen virksomhedsvalg, filstier eller nøgler. Resultatet er
`{ok:true,backup:{contentType,bytes}}` med serverens præcise signerede fil,
begrænset til 96 MiB; klienten påstår ikke selv at have verificeret eller
gendannet den.

## Budget, moms og perioder i desktop

**Budget** viser serverens regnskabsperioder, budget mod faktiske tal og
dimensionsfiltre. En ændring fastholdes til gennemgang, bekræftes særskilt og
læses tilbage fra serverens budgetrevisioner. Appen beregner ingen budgettal.

**Moms og perioder** viser momsopgørelsen og den fulde aktuelle kontrolpakke.
Review og lukning er særskilte handlinger; lukning kræver samme review-ID og
pakkehash. Genåbning kræver en begrundelse og en lukket, ikke indberettet
periode. Registrering af en allerede gennemført ekstern indberetning bruger
dens faktiske kvitteringsreference og periodens oprindelige lukkebevis.

Uafklarede svar eller manglende kanonisk genlæsning bevarer en privat
handlingsblokering. Nyt lokalt portnummer accepteres kun for samme profil,
bruger, ledger og periode; fjernforbindelser skal fortsat stemme præcist.
De samlede 116 desktop-tests og build består. Faktiske vinduesforløb er endnu
ikke verificeret; den faktiske accept på den nu oplåste Mac er i gang.

## Registrér og afstem rykkergebyrer

Fakturaens **Rykkergebyrer og krav** registrerer et eksplicit valgt gebyr på
en forfalden faktura. Registreringen sender ingen mail og bogfører intet.
Særskilt gennemgang og bekræftelse bogfører gebyret på de valgte konti.
Efter hovedstolen er betalt kan indgående DKK-bankposter afvikle gebyrkravet
delvist eller helt. Originaler og afstemningsrelationer kontrolleres, og
fremmede, uvaliderede fakturaposteringer blokerer fortsat afstemningen.

CLI og MCP har læsning, registrering, bogføring og kravafvikling gennem den
samme Rust-logik. Det faktiske første web-/klientforløb med fem posteringer,
originaler og nødvendige afvisninger består. Det efterfølgende periodetjek
afslørede en forældet afvisning af kravbetalinger; rettelsen kontrollerer nu
de faktiske posteringer og entydige auditspor. Den samlede faktiske førstegangs- og genstartskontrol består nu også.
Mailafsendelse, renter og kompensation er fortsat selvstændige paritetskrav.

## Virksomhedsoplysninger og parter

Virksomhedsoplysninger kan foreslås, godkendes, afvises og erstattes med
kilde, gyldighed og bevaret historik gennem web, CLI og MCP. Den signerede
backup bevarer hele dette register; faktisk gendannelse og genstart med
uafhængigt verificerede web-, CLI- og MCP-arkiver består.

Workspace-parter kan registreres med identifikatorer, virksomhedsroller og
kildebaserede aliaser og feltudsagn. Visning følger de aktuelle
virksomhedsrettigheder. Globale ændringer kræver de nødvendige rettigheder
til alle partens tilknyttede virksomheder. Den oprindelige identitet og
historik bevares. Web og de seks tilsvarende klientoperationer er integreret;
det faktiske browser-/klientforløb og genstart består. Den signerede
backupkode bevarer alle seks tabeller med partshistorik og kilder. Faktisk
gendannelse og genstart med partbaserede virksomhedsoplysninger består.
Web-, CLI- og MCP-arkiver er hver verificeret med den separate offentlige
nøgle, efter at kildeworkspacet og den private nøgle er fjernet.

## Signeret lokal desktop-backup

En ny administreret lokal profil opretter en privat signeringsnøgle uden for
workspacet. Desktopappens **Workspace-backup** kan hente arkivet og eksportere
den offentlige nøgles 32 bytes særskilt til en ny, eksplicit valgt fil.
Nøglen skal bevares uafhængigt af arkivet til senere verificering.
Eksisterende profiler får ikke automatisk udskiftet eller tilføjet nøgler;
fjernserverens offentlige nøgle leveres af administratoren.
Tests og build kvalificerer implementeringen; de nye faktiske vinduesforløb
afventer det relevante faktiske vinduesforløb på den nu oplåste Mac.


## Virksomhedsoplysninger i desktop

**Virksomhedsoplysninger** viser serverens aktuelle kontekst, konflikter,
kilder og hele historikken. Forslag og erstatninger bruger eksplicitte
JSON-værdier, kilder og gyldighedsdatoer. Gennemgang og bekræftelse er
særskilte trin. En erstatning er et nyt forslag, som godkendes særskilt.
Appen kontrollerer den aktuelle historik før ændringen og genlæser serverens
resultat; uafklarede udfald kræver manuel afklaring. De samlede native tests
og build består; faktiske nye vinduer er endnu ikke verificeret.


## Parter i desktop

**Workspace-parter** viser serverens synlige parter med søgning, sider,
identifikatorer, virksomhedsroller, kildeudsagn og historik. Oprettelse,
rolletilknytning og nye aliaser eller feltudsagn kræver en eksplicit
kildebaseret gennemgang og bekræftelse. Den oprindelige identitet overskrives
ikke. Serveren afgør rettighederne, og appen genlæser det kanoniske resultat.
De samlede 116 native tests og build består; faktiske nye vinduer er endnu
ikke verificeret; den faktiske accept på den nu oplåste Mac er i gang.


## Planlagte fakturaer på serveren

En server kan udstede dagens normale forekomst uden en åben browser. Det kræver
både en særskilt servicekonto, en udtrykkeligt startet serverarbejder og ejerens
gennemgang af den konkrete fakturaskabelon. Udstedelse gemmer JSON-original og
PDF; bogføring og afsendelse er fortsat særskilte handlinger.

1. En workspace-ejer med frisk MFA åbner **Brugere og adgang → Servicekonti**
   og vælger **Opret servicekonto**. Gem engangsnøglen i operatørens
   hemmelighedshåndtering. Den kan ikke hentes igen. Aktivér servicekontoens
   workspace-adgang som medlem og giv den eksplicit rollen **Bogholder** i
   den relevante virksomhed under **Ændr adgang**. Oprettelse af nøglen giver
   ingen virksomhedsadgang i sig selv.
2. Operatøren genstarter den konfigurerede server med de to sammenhørende flag
   nedenfor. Processtyringen skal levere præcis én servicenøgle på stdin,
   eventuelt efterfulgt af ét linjeskift, og derefter lukke inputstrømmen.
   Nøglen må ikke stå i argumenter eller workspace-konfigurationen. Serveren
   beholder kun nøglens opslagshash i hukommelsen. En ny proces kræver ny
   injektion af nøglen. Flagene kan ikke kombineres med oprettelse,
   backupverifikation eller gendannelse.

   ```sh
   rentemester-server --config /sti/til/server.json \
     --listen 127.0.0.1:4310 --web-dir /sti/til/web \
     --scheduled-worker --scheduled-service-key-stdin
   ```

3. Aktivér eller pausér den ønskede planlægning, og afklar først manglende
   forekomster, kø og fejl. Under **Planlægning på serveren** vælges
   **Gennemgå serverplan og servicekonto**. Gennemgå sælger, kunde, linjer,
   interval og den faktisk konfigurerede servicekonto. Angiv begrundelse,
   bekræft og vælg **Godkend automatisk udstedelse på serveren**.
4. Browseren kan nu lukkes. Serveren undersøger højst 32 kandidater pr. omgang
   og venter 30 sekunder efter en afsluttet omgang. Den udsteder kun dagens
   normale forekomst. Manglende perioder, uafklarede forsøg og fejl kræver
   gennemgang; dvale og genstart udløser ingen automatisk indhentning.
5. **Tilbagekald serversamtykke og sæt på pause** fungerer også, hvis serveren
   genstartes uden servicenøglen. Almindelig pause eller annullering ophæver
   også serversamtykket. **Genoptag** giver kun appens tilstand tilbage;
   serverkørsel kræver et nyt gennemgået serversamtykke.

Hver udstedelse kontrollerer servicekontoens aktuelle nøgle og virksomhedsadgang
samt den samtykkende ejers aktuelle verificering, MFA og ejeradgang. Rotation,
tilbagekaldelse og ændring af adgang kan ikke genoplive et gammelt samtykke ved
blot at tildele adgangen igen. Servicekontoen står som aktør på udstedelsen og
kørslen; ejerens samtykke registreres særskilt. Et registreret samtykke er ikke
en måling af, om serverarbejderen er i drift.

Appens og serverens automatik kan ikke køre samme planlægning samtidigt.
Den administrerede lokale desktopserver starter ikke denne arbejder. Læsning,
login, genåbning og almindelig gendannelse aktiverer ingen arbejder.
Signerede V3-backups bevarer historik og originaler, men gendannelse suspenderer
serversamtykket uafhængigt af det almindelige gendannelsessamtykke. En ny
servicekonto og et nyt gennemgået serversamtykke kræves efter gendannelse.

CLI tilbyder `service-principals`, `create-service-principal`,
`rotate-service-principal`, `revoke-service-principal`,
`recover-service-principal`, `hosted-recurring-plan`,
`consent-hosted-recurring` og `revoke-hosted-recurring`. De tilsvarende MCP-navne
bruger understregninger. Skrivninger bruger eksplicit bekræftet JSON på stdin;
kommandoernes hjælpetekst indeholder de præcise skemaer. Oprettelse og rotation
returnerer bevidst engangsnøglen, så deres output skal behandles privat.
Efter et mistet svar skal det oprindelige operations-ID afklares med
`recover-service-principal`; oprettelse og rotation gentages ikke automatisk.


## Login og gendannelse på en HTTPS-server

Start Rust-serveren bag HTTPS med en eksplicit kanonisk origin, fx
`--listen 127.0.0.1:4319 --origin https://regnskab.example.invalid`.
Proxyen skal bevare denne origins Host-header. Serveren bruger samme
normaliserede origin til login, produktkald og links; klientstyrede
forwarded-headere vælger ingen anden origin. HTTPS vælger sessionscookies
med `Secure`, `HttpOnly` og `SameSite=Lax` samt de sikre cookienavne.
HTTP er kun tilladt på loopback.

HTTPS kræver `authMail` i den private serverkonfiguration:

```json
{
  "authMail": {
    "endpoint": "https://mailgateway.example.invalid/auth-mail",
    "bearerToken": "OPERATOERENS_HEMMELIGHED",
    "from": "adgang@example.invalid",
    "idempotencySecret": "OPERATOERENS_SEPARATE_HEMMELIGHED"
  }
}
```

Gatewayen modtager JSON-formatet `rentemester-auth-email-v1` med `kind`,
`to`, `from`, `subject`, `text` og `actionUrl`. Den skal håndtere
`idempotency-key`, som er stabil for samme mailtype og token. `kind` er
`verification`, `workspace-invitation` eller `password-reset`.
Bearer-headeren autentificerer leveringen. Et 2xx-svar bekræfter overdragelse
til gatewayen; faktisk levering til postkassen afhænger af gatewayen.
En privat gateway kan vælge `trustedCaFile`, en absolut sti til højst
64 KiB PEM-certifikater med dens betroede CA. Certifikat- og værtsnavnekontrol
bevares. Uden feltet anvendes de almindelige indbyggede tillidsrødder.
Ugyldig eller manglende gatewaykonfiguration afviser HTTPS-opstart før
oprettelse af workspace.

Første ejer og inviterede brugere modtager verificeringslinket gennem denne
adapter og opsætter derefter MFA i den eksisterende webapp. **Glemt
adgangskode?** sender et reset-link. Linkets åbning ændrer ingen adgangskode;
brugeren indtaster og bekræfter den nye adgangskode. En gennemført nulstilling
ophæver brugerens tidligere sessioner og forbruger engangstokenet. Brugeren
logger derefter ind igen med den nye adgangskode og sin eksisterende MFA.

Mailtoken og historik gemmes før gatewaykaldet. Et timeout kan derfor følge
efter faktisk overdragelse. Serveren beholder det allerede udstedte token,
og den offentlige reset-kvittering afslører ikke, om kontoen findes eller
mailen er modtaget. Gatewaykald har et samlet budget på otte sekunder og
begrænsede genforsøg med samme idempotency-nøgle.

## Pak desktopappen uden gamle filer

`crates/rentemester-desktop/build-macos.sh` samler appen i en ny stagingmappe
og erstatter kun en eksisterende Rentemester-bundle. `CARGO_TARGET_DIR`,
`CARGO_BUILD_TARGET` og `RENTEMESTER_DESKTOP_PROFILE` kan vælge buildplacering,
platform og profil. De byggede webfiler skal være klar før kommandoen køres.
Appen indeholder begge Rust-programmer og webfilerne; Python bruges kun til
samlingen. Filen `Contents/Resources/native-artifact-manifest.json`
registrerer repository-revision, dirty-status, værktøjskæder, profil og
SHA256 for de faktiske filer. Pakken er fortsat usigneret; installation via
Finder, signering og notarization kræver særskilt verifikation.


## Send udstedte fakturaer gennem SMTP

Rust-serveren bruger den udstedte fakturas gemte PDF. Åbn **Fakturaer →
Maillevering**, vælg faktura, modtager og mailtype, gennemgå PDF’en og
bekræft afsendelsen. Afsendelse bogfører ikke fakturaen. Mailtypen
`reminder` ændrer mailteksten; den opretter ingen rykkerpostering eller renter.

Operatøren angiver SMTP pr. virksomhed i den private serverkonfiguration:

```json
{
  "invoiceMail": {
    "example-company": {
      "host": "smtp.example.invalid",
      "port": 465,
      "fromAddress": "faktura@example.invalid",
      "fromName": "Eksempelvirksomhed",
      "username": "SMTP_BRUGER",
      "password": "OPERATOERENS_HEMMELIGHED",
      "tls": "implicit"
    }
  }
}
```

`tls` kan være `implicit` (standard) eller påkrævet `starttls`. Forbindelsen
kontrollerer certifikat og værtsnavn; der er ingen ukrypteret fallback.
`trustedCaFile` kan vælge en absolut PEM-sti på højst 64 KiB til en privat CA.
Konfiguration og adgangskoder holdes uden for virksomhedens workspace og
backup. Uden konfiguration kan historikken stadig læses og ukendte forsøg
afklares; afsendelse og nye forsøg afvises med `smtp_config_required`.

Før netværkskaldet gemmer motoren en reservation. Kun SMTP-serverens endelige
positive svar registreres som transportaccept; det beviser ikke levering til
modtagerens postkasse. En udtrykkelig SMTP-afvisning giver **afvist**, mens
timeout, forbindelsestab eller nedbrud efter reservation giver **ukendt**.
Ét forsøg har et samlet netværksbudget på otte sekunder og ingen automatiske
genforsøg. Gentaget almindelig afsendelse sender aldrig igen.

Ved **ukendt** undersøger ejeren SMTP-loggen og registrerer udfald,
begrundelse og dokumentation. Afklaringen sender ingen mail. En menneskelig
accept vises særskilt fra en SMTP-kvittering. Kun et afvist forsøg kan sendes
igen gennem en separat gennemgang med begrundelse, PDF og bekræftelse.
Historiske afsendelsesregistreringer vises som historiske og deduplikeres.
Genstart og backupgendannelse bevarer historikken uden at sende noget.

API’et under `/api/companies/{slug}/invoices` har `GET email-deliveries`,
`POST send-email`, `POST email-deliveries/reconcile` og
`POST email-deliveries/retry`. Skrivekald kræver bekræftelse, aktuel
menneskelig MFA og rettigheden `CompanyExternalSend`. De kan returnere den
kanoniske `delivery` sammen med HTTP 422 `delivery_refused` eller HTTP 409
`delivery_unknown`. Ved tabt HTTP-svar læses historikken før videre handling.
CLI har `invoice-email-status`, `invoice-email-send`,
`invoice-email-reconcile`, `invoice-email-retry`; MCP bruger tilsvarende navne
med understregninger. Skriveinput sendes som JSON på stdin med `confirm:true`.


Den integrerede syntetiske macOS-test har gennemført almindelig ejeroprettelse
og MFA, webafsendelse, CLI-dublet uden SMTP-forbindelse, SMTP-afvisning og
særskilt godkendt genforsøg. Et reelt servernedbrud efter testserverens
durable modtagelse af DATA blev fulgt af genstart, MCP-dublet uden genafsendelse
og ejerens dokumenterede afklaring. De modtagne MIME-, message-ID- og PDF-hashes
matcher historikken, og bogføring samt originaler er uændrede. En signeret
backup blev verificeret uden kilden og den private signeringsnøgle og
gendannet med ny ejer/MFA uden SMTP-konfiguration; historik og originaler
blev bevaret uden afsendelse. Testen videresender ingen mails og kvalificerer
ikke en virkelig SMTP-udbyder, modtagernes postkasser eller produktionsdrift.


## Flyt workspace mellem lokal drift og server

Åbn **Workspace → Flyt workspace** på kilden. Gennemgå destinationen og
begrundelsen, og bekræft frysning. Kilden afviser nu almindelige ændringer,
bogføring og automatisk afsendelse. Login, autoriserede læsninger og eksport
bevares. Frysningen ligger i workspace-data og overlever genstart og en
kopieret serverkonfiguration. Før et flyttearkiv er tilbudt, kan ejeren
annullere med en begrundelse. Efter eksport kan kilden ikke åbnes for
skrivning igen gennem denne handling.

Dan det signerede flyttearkiv fra samme side. Serveren registrerer arkivets
SHA256, før downloadsvaret frigives. Gem også den separat betroede offentlige
kildenøgle; en nøgle i et modtaget dokument etablerer ingen tillid.

Operatøren klargør et **nyt** mål med ny privat konfiguration, nye cookie- og
MFA-nøgler, ny `firstOwner`, `restoreActor` og `transferTrustedKeyFile`, som
peger på den separat kontrollerede offentlige kildenøgle. Start eksempelvis:

```sh
rentemester-server --config NEW_PRIVATE.json \
  --restore-backup workspace.transfer.native-backup.json \
  --trusted-backup-key SOURCE_PUBLIC_KEY \
  --confirm-target EXACT_CONFIG_ROOT --restore-transfer
```

`--restore-transfer` kan ikke kombineres med `--source-unavailable` eller
`--create`. Arkivet verificeres før publicering, og målet får en ny lokal
instansidentitet og en varig **klargjort** tilstand. Det er endnu ikke en
skriver. Første ejer verificerer sin e-mail og opsætter MFA som normalt.
Almindelig gendannelse kan ikke bruges til at omgå en frosset eller klargjort
flytning.

Målets flytteside viser en udfordring bundet til arkivet, workspace,
virksomhedernes ledger-identiteter, målidentiteten og den kanoniske origin.
Kontrollér målet gennem en uafhængig kanal. Gennemgå derefter udfordringen på
kildens flytteside og frigiv til netop dette mål. Frigivelsen gemmes før
svaret og kan læses igen efter et tabt svar. Kilden bliver permanent
skrivebeskyttet. Det betyder, at målet er autoriseret; det beviser ikke,
at målet faktisk er aktiveret.

Indsæt den signerede frigivelse på målet, kontrollér den separat konfigurerede
nøgle, og bekræft aktivering. Kun målserverens kanoniske **aktiv**-tilstand
tillader almindelige ændringer. Andre instansidentiteter, arkiver og nøgler
afvises. Aktivering starter ingen mails eller planlagte fakturaer. Gamle
servicenøgler og eksekveringssamtykker giver ingen ny myndighed. Ukendte
SMTP-forsøg bevarer deres historik og kan ikke automatisk sendes igen.
Et aktivt mål kan senere starte en ny, udtrykkeligt godkendt flytning med nye
flytteidentiteter; den tidligere kilde forbliver skrivebeskyttet.

Hvis en bogføring blev accepteret på kilden, men svaret gik tabt, kan den nye
ejer gennemgå den importerede kvittering og tilknytte den til sin aktuelle
bruger med en begrundelse. Det ændrer ingen historisk bruger og bogfører
intet. Efter aktivering kan den nøjagtige oprindelige nøgle og payload returnere
det bevarede resultat med `receipt.replayed:true` og afklaringsproveniens.
Aktuelle rettigheder kontrolleres stadig først. Ændret payload, manglende
resultat, tvetydig kvittering eller manglende tilknytning afvises. Denne
afklaring understøtter aktuelt `journal_post` og `journal_reverse`; øvrige
nøglebaserede operationer er fortsat et paritetskrav.

Ved et tabt aktiveringssvar læses tilstanden igen. Der er ingen automatisk
optøning eller tilbageførsel til den gamle kilde. Gendannelse efter ny aktivitet
på målet må bevare denne aktivitet; et fuldt efterfølgende rollback-forløb er
fortsat åbent. Den lokale protokol kan ikke forhindre en betroet operatør i
at starte en uafhængig kopi fra før frysningen eller bruge ældre software,
der ignorerer spærren. Der hævdes ingen global beskyttelse mod vilkårlige
kopier eller produktionskvalifikation på tværs af maskiner.

Dette administrerede flytteforløb er verificeret på syntetiske data med to
macOS-loopbackservere: almindelig ejer/MFA på begge, særskilt signaturkontrol,
kvitteringstilknytning uden dobbelt bogføring og genstart med kilden fortsat
skrivebeskyttet. Et reelt afbrudt SMTP-forsøg forblev uafklaret uden genafsendelse.
Testen kvalificerer ikke de ovennævnte åbne rollback-/kopigrænser eller en
installeret desktopapp. Se `implementation/progress.json` under Rust-målarkitekturen
for revisionsbundne beviser og resterende arbejde.

## Bogfør flere køb efter en gennemgået plan

Åbn **Bogføringskø**, vælg perioden, og hent arbejdskøen. Et køb kræver det
oprindelige bilag, bankkilden, en udtrykkelig leverandørtilknytning og en
uafhængigt godkendt købsregel. Køen viser alle kandidater og blokeringer;
et filter gør ikke blokerede køb bogføringsklare.

Gennemgå bilag, leverandør, konto, moms og den præcise plan. Gem planen efter
særskilt bekræftelse. En anden bruger gennemgår og godkender samme plan;
derefter kan den anvendes med en ny bekræftelse. Ændrede kilder kræver en ny
plan. Nye Rust-planer anvender `atomic_v1`: enten gemmes alle posteringer,
kvitteringer og afsluttende kontroller, eller også tilbageføres hele forsøget.
Tidligere TypeScript-planer med delvis udførelse bevares som historik; deres
udførelsesparitet er fortsat åben.

Ved et tabt svar læses kørslen med dens id eller den præcise kørselsnøgle.
Der sendes ikke automatisk igen. Gentagen anvendelse af samme godkendte plan
returnerer de oprindelige kvitteringer uden nye posteringer. CLI tilbyder
`bookkeeping-batch-plan`, `bookkeeping-batch-status`,
`bookkeeping-batch-persist`, `bookkeeping-batch-approve` og
`bookkeeping-batch-apply`; MCP bruger tilsvarende navne med understregninger.
Skrivekald kræver JSON med `confirm:true` og eksplicit bekræftelse i klienten.

Det syntetiske webforløb er verificeret med to almindeligt inviterede brugere
med MFA, to originale danske køb, bankimport, leverandørtilknytning,
uafhængig regel- og plangodkendelse, bogføring, moms og bankafstemning.
HTTP, CLI og MCP giver samme plan, status og gentagne kvitteringer. Genstart
og gendannelse fra en selvstændigt verificeret signeret backup med ny ejer og
MFA bevarer originaler, leverandørhistorik, posteringer og godkendelser.
Testen kvalificerer denne DKK-købsfamilie; øvrige moms-/udtræksfamilier,
installeret desktop og den samlede TypeScript-udfasning er fortsat åbne.

## Ret et bankkøb til en leverandørfaktura

Åbn **Leverandørfakturaer → Ret direkte bankkøb**, hvis et køb allerede er
bogført direkte mod bank, men skal vises som en faktura med en senere betaling.
Vælg det eksisterende køb, udgiftskonto, momsbehandling, forfaldsdato og
begrundelse. Fakturadatoen kommer fra originalbilaget; bankdatoen kommer fra
den tilknyttede bankpost. Almindelig fakturaregistrering accepterer fortsat
ikke allerede bogførte bilag.

Åbn originalbilaget og den oprindelige postering, og gennemgå datoer, beløb,
konto, moms og den fulde plan-hash. Bekræft gennemgangen og korrektionen.
Serveren bevarer originalen og tilføjer samlet en tilbageførsel, en
kreditorpost på fakturadatoen og en betaling på bankdatoen. Den aktuelle
bankrelation flyttes til betalingen. Hele forsøget tilbageføres ved fejl.

Planen læses med `POST payables/direct-bank-correction/plan`; anvendelse sker
med `POST payables/direct-bank-correction/apply`. Begge modtager højst 16 KiB
JSON. Anvendelse kræver den gennemgåede plan-hash, begrundelse, `confirm:true`
og en udtrykkelig idempotensnøgle. Header- og bodynøgle skal være ens, hvis
begge sendes. Samme nøgle og indhold giver den bevarede kvittering; ændret
indhold giver HTTP 409. Ved et tabt svar kontrolleres kreditorposter,
posteringer og bankhistorik før en ny handling. Der sendes ikke automatisk igen.

CLI har `direct-bank-payable-correction-plan` og
`direct-bank-payable-correction`, begge med præcist JSON på stdin; kun
anvendelse kræver `--confirm`. MCP har tilsvarende navne med understregninger
og en nøjagtig JSON-streng som `body`. Planen er en læsning uden bekræftelse.

Det syntetiske forløb er verificeret gennem web, CLI og MCP, genstart med MFA
og signeret backup gendannet under en ny ejer efter fjernelse af kilden og
den private signeringsnøgle. Købet står åbent på fakturadatoen og betalt på
bankdatoen, mens endelige udgifter, moms og bankbeløb bevares. Historiske
brugere og kvitteringer ændres ikke ved gendannelsen. Denne test kvalificerer
ikke genbrug af importerede korrektionsnøgler under den nye ejer eller en
installeret desktopapp.

## Gennemgå mellemregninger mellem selskaber

Det nye native forløb under koncernoverblikket samler dispositionens parter,
originale dokumentation, særskilte selskabsposteringer og afviklingsevidens.
Group59 har bestået den samlede browser-, genstarts- og backupprøve med to
almindelige brugere, særskilt bogførte selskabsposteringer og en ny ejer
efter gendannelse. Både kildeworkspace og dets private signeringsnøgle blev
fjernet før uafhængig verifikation og gendannelse. Det gendannede workspace
er også genstartet uden nye økonomiske handlinger. Installeret desktop og
de øvrige dele af omskrivningen er fortsat uafsluttede.

Opret først en læsende plan og behold planens returnerede disposition-ID.
Gennemgå beløb, dato, selskaber, dokumentation og hele payload-hashen, før
forslaget gemmes. En anden bruger med aktuel adgang til begge selskaber skal
godkende den samme disposition. Bogfør derefter begge selskabers faktiske
posteringer særskilt, før nogen af dem tilknyttes. Tilknytningerne binder
postering og ledger-head; senere bogføring kan derfor gøre kilden forældet.
Status viser sådanne problemer uden at ændre eller opfriske beviset.

Registrering af afviklingsevidens foretager ingen betaling. Tilstanden
`settled` har `balanced:false`; kun `posted` uden undtagelser har
`balanced:true`. Erstatning og genåbning kræver en begrundelse og bevarer
historikken. Ingen af disse handlinger bogfører eller eliminerer beløb.

CLI har det globale læsekald `remote group-disposition-status --id ID`
med valgfrit `--as-of YYYY-MM-DD`. De syv handlingskald hedder
`plan-group-disposition`, `propose-group-disposition`,
`approve-group-disposition`, `link-group-disposition`,
`settle-group-disposition`, `supersede-group-disposition` og
`reopen-group-disposition`. De bruger `--company` som ankerselskab og
JSON på stdin, højst 32 KiB. Planen er læsende og kræver ingen bekræftelse;
ændringer kræver `--confirm` og `confirm:true` i JSON. MCP bruger tilsvarende
navne med underscores. Se CLI-hjælp og MCP-skemaer for de præcise felter.
Ved ukendt udfald beholdes ID og anmodning; læs status før næste handling.
Der er ingen automatisk genafsendelse eller generering af nye nøgler.

Den eksisterende signerede sektion `control/intercompany.json` udvides til
version 2 med dispositioner, begivenheder, posteringstilknytninger og
livscyklushistorik. Version 1 læses med tom dispositionshistorik. Gendannelse
skal validere de oprindelige journalbeviser og bevare historiske aktører uden
at give dem aktuel adgang.

## Importér kunder og leverandører fra Dinero

Den eksisterende kontaktvisning bruger `Importér` og filen `Kontakter.csv`.
Det native Group60-forløb har bestået den samlede browser- og backupprøve
med almindelig MFA, faktiske syntetiske HTTPS-opslag, genimport gennem web,
CLI og MCP uden ekstra kontakter, audit- eller providerkald, samt genstart.
Signerede snapshots bevarer kontakter og CVR-cache uden providercredentials;
gendannelse hos en ny ejer efter fjernelse af kilde og privat nøgle,
genimport og endnu en genstart består også. Importen opretter kunder og leverandører ud fra
filens salgs- og købshistorik og viser oprettelser, eksisterende kontakter og
rækkefejl. Den opretter ingen posteringer. Genimport bruger den præcise
kombination af navn og skatte-ID i hver kontakttabel og ændrer ikke allerede
oprettede kontakter. EAN lagres som kontaktdata; dette aktiverer ikke Peppol.

Valget om CVR-berigelse er fortsat synligt og slået til fra begyndelsen.
CSV-felter har forrang. Serveren bruger først et aktuelt CVR-snapshot; et
providerproblem kan bruge et ældre bevaret snapshot. Uden snapshot og et
gyldigt opslag importeres CSV-data med en synlig fejl ved berigelsen.
Fejltælleren gælder kontaktroller, så en kontakt med begge roller kan have
to fejl. Ingen vellykket berigelse påstås, når opslaget fejler.

Operatøren konfigurerer `CVR_USERNAME`, `CVR_PASSWORD` og valgfrit
`RENTEMESTER_CVR_ENDPOINT` i serverprocessens beskyttede miljø.
Standardendpoint er `https://distribution.virk.dk/cvr-permanent`.
`RENTEMESTER_CVR_TRUSTED_CA_FILE` kan pege på en privat, betroet CA-fil.
Endpoint og credentials modtages ikke fra import-JSON, gemmes ikke i
kontakternes cache og følger ikke med backup. Transporten kræver verificeret
HTTPS, bruger højst 12 sekunder og 4 MiB svar pr. opslag og følger hverken
redirects eller automatiske genforsøg. Højst 64 MiB rå CVR-data beholdes pr.
import; derefter bliver resterende opslag til synlige berigelsesfejl eller
brug af et faktisk ældre snapshot, mens CSV-data stadig kan importeres.

CLI-kaldet er `remote import-contacts --company SLUG --confirm` med de
almindelige eksplicitte server- og cookievalg. Stdin er serverens præcise
JSON `{fileName,content,enrichCvr?,confirm:true}`, højst 12 MiB. MCP hedder
`import_contacts` med `company`, den samme JSON som `body` og `confirm:true`.
Klienterne bevarer hele svaret, inklusive delvise rækkefejl. Ved ukendt
udfald skal kontaktregistret læses før en ny eksplicit import; der findes
ingen automatisk genafsendelse eller opfundet kvitteringsnøgle.
