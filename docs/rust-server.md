# Kør Rust-serveren med web-UI'et

Rust-serveren leverer det eksisterende React-cockpit og behandler selv login,
ejeroprettelse, MFA, sessions, virksomhedsadgang, manuel bogføring, tilbageførsel,
virksomhedsstamdata, kontooprettelse, bekræftelse af kontoroller, bankkonti, generisk bankimport, bilagsbogføring med valutakøb samt bilagsindlæsning og resultatopgørelse, balance og saldobalance med CSV/PDF-eksport samt momsoverblik med PDF, kundeoprettelse, udstedelse/bogføring af danske salgsfakturaer samt betalinger, kreditnotaer, afstemning af kundetilbagebetalinger og leverandørkort med bilagsudfyldning samt leverandørgæld med senere bankbetaling. Den
starter ikke TypeScript-backenden og sender ikke forespørgsler videre til den.

## Aktuel Rust-levering

[Dinero-import og importerede tilgodehavender](rust-dinero-import.md) har nu
en fælles kildepakke, gennemgang og atomisk bogføring i Rust. Se vejledningen
for format, adgang, gentagelser og resterende importvarianter.

[Konstateret tab](rust-invoice-bad-debt.md) fungerer nu på DKK- og valutafakturaer
gennem web, CLI og MCP. Delvise tab, resterende hovedstol, separate krav og
faktiske indbetalinger bevarer korrekt DKK-værdi og moms. Prøven med mistet
svar uden automatisk gentagelse, aktuelle rettigheder, backup-lås, fuld
regnskabskontrol, nyt MFA-login efter genstart og uafhængigt verificeret
signeret backup består på syntetiske data.

[Renter og kausale rettelser](rust-invoice-interest.md) fungerer nu i DKK gennem
server og fælles web-UI: separat beregning, registrering og bogføring,
tilbagedateret delbetaling, konkret korrektion og faktisk renteindbetaling.
Prøven med almindelig MFA, aktuelle rettigheder, backup-lås, præcis CLI/MCP-
genlæsning og afvisninger, fuld regnskabskontrol, genstart og uafhængig
verificering af signeret backup består. Renter på valutafakturaer har en
dokumenteret afvisningskontrakt som i kildens bogføringsregel; den øvrige
omskrivning er fortsat åben. Læs vejledningens afgrænsning.

[Kompensationskrav](rust-invoice-compensation.md) har nu særskilt beregning,
registrering og bogføring i Rust med fælles web-UI, CLI og MCP. Hovedstol i
fakturavaluta og kompensationskrav i DKK behandles hver for sig, også ved
bankbetaling. Forløbet kræver en aktuel menneskelig MFA-session.

Virksomhedens [revisionshistorik](rust-audit-log.md) kan nu filtreres og læses
med fuld pagineringsmetadata gennem web, HTTP, CLI og MCP. Aktuelle menneske-
og servicekontorettigheder håndhæves; opslag efter bogføring/tilbageførsel,
læserrevokation og normal-MFA-genstart er verificeret på syntetiske data.
Den særskilte [regnskabskontrol](rust-audit-verify.md) er nu tilgængelig gennem
web, HTTP, CLI `remote audit-verify` og MCP `audit_verify` (257 værktøjer).
Journal, bank, fakturagrundlag, beskyttede originaler samt schema/SQLite har
hver sit resultat. Manglende kontrol bliver ikke vist som et rent regnskab.
En almindelig MFA-prøve med bogføring/tilbageførsel, to virksomheder, aktuelle
menneske-/servicerettigheder, webvisning, ændrede ubogførte bilags- og bankoriginaler
og genstart er verificeret på syntetiske data. Læs vejledningens kildebegrænsninger;
hele den resterende regnskabs-, valuta-, lifecycle- og restorematrix, manglende
native skriveforløb og endelig desktopaccept er fortsat åbne.

Signeret gendannelse på en frisk instans bevarer nu også backupregistreringer,
uverificerede erklæringer og fjernelseshistorik. Prøven fjerner den oprindelige
testinstans og private nøgle først og verificerer begge virksomheders data via
HTTP, CLI og MCP med ny MFA-ejer og genstart. Gammel backup- eller
providerautoritet bliver ikke aktiv på den nye instans. Se
[gengivelsen af forløbet](rust-backup-destinations.md#gendannelse-uden-den-oprindelige-instans).

Virksomhedens backupstatus og frivillige finansielle bogføringslås fungerer nu
på tværs af web, HTTP, CLI og MCP samt servicekonti. Faktisk normal-MFA-prøve
med mistede/ændrede filer, frisk backup, én postering og genstart består.
Kontrolbaserede metadata- og indbakkeruter mangler endnu fuld låseparitet;
se [backupkontrakten](rust-backup-destinations.md).

Konfigureret backupplacering og genlæsning af samme signerede arkiv fungerer nu
gennem server, web, CLI og MCP med genstart. Se [opsætning og afgrænsning](rust-backup-destinations.md).
Placeringen er en lokal/synkroniseret mappekopi. Den valgfrie Drive-kontrol
kan derefter kontrollere metadata og genlæse den eksterne fils præcise bytes
med operatørens token og mappebinding. Kvitteringer, mistet svar, aktuelle
rettigheder før/efter netværkskald og genstart er afprøvet med en privat
TLS-protokolfixture gennem web, CLI og MCP. Ingen virkelig Google-konto eller
offsite-drift er dermed kvalificeret. Den samlede virksomhedsspecifikke backupvisning
sammenholder lokal friskhed og lås med placerings- og eksterne kvitteringer for
præcis samme arkiv; en nyere backup overtager ikke en ældre kvittering.
Ejeren kan desuden registrere egne opbevaringssteder, notere eksplicit
uverificerede overførsler og fjerne aktive registreringer med bevaret historik.
Alle fem destinationstyper er metadata; serverens fysiske mål og credentials
forbliver operatørstyrede. Øvrig health/audit-paritet, uafhængig fuld gendannelse
af den nye registreringshistorik og de samlede releasegates er fortsat åbne.

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

Der er 202 MCP-tools efter fakturaudtrækket i Group70.
De samlede native tests består med 233 beståede og én eksisterende ignoreret
test; native build består. Computerstyringen melder aktuelt Mac’en låst. Administreret
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

## Importér et kontoudtog med Danske Bank-profilen

Åbn **Bankkonti**, opret eller vælg den relevante bankkonto, og vælg
**Importér kontoudtog**. Angiv kontoen og profilen `danske-bank`, vælg bankens
CSV i UTF-8 eller Windows-1252, gennemse oplysningerne og bekræft importen. Profilen fortolker
bankens datoer som dag–måned–år og genkender de deklarerede kolonner.
Uden profil bruges den eksisterende generelle parser, som afviser tvetydige
datoer.

Importen bevarer afsender, modtagerkonto, besked, arkiv- og kundereference
samt alle oprindelige kolonnenavne og deres parsede celleværdier. Det præcise
filindhold gemmes separat som original, inklusive BOM og linjeskift.
Browseren sender filbytes som base64 uden tekstkonvertering. Serveren dekoder
UTF-8 med valgfrit BOM; profilen `danske-bank` bruger Windows-1252, hvis UTF-8
ikke kan dekodes uden erstatningstegn. Uden profil kræves gyldig UTF-8.
Filens SHA-256 beregnes af originale bytes. To kodninger af de samme logiske
bankposter kan have forskellige filhashes, men samme transaktionsidentitet og
dubletforekomster. Originalen bevares også ved en import med kun dubletter.

Rækkefølgen samme dag udledes af bankens saldi. Et saldospring vises som
advarsel og gemmes som `BANK_BALANCE_GAP`; de gyldige bankposter bevares.
En gentaget import springer allerede kendte poster over uden ny bogføring
eller ny saldoadvarsel. Et normalt importforsøg får fortsat sit eget auditspor.
Der gættes ingen valutakurs eller bankkonto.

HTTP, CLI `import-bank` og MCP `import_bank` bruger den eksisterende
importoperation med `profile:"danske-bank"`. Send præcis ét af `csvContent`
(den eksisterende tekstindgang) eller `csvBase64` (kanonisk standard-base64 af
originale bytes), samt `confirm:true` og valgfri `account`/`profile`. Begge
kilder, ingen kilde, null i kildefeltet, ugyldig base64 og overskredne grænser afvises.
Originalen og den afkodede UTF-8-tekst må være højst 25 MiB, og JSON-body
højst 35 MiB. CLI bruger stdin;
MCP bruger den eksisterende `body`-streng og ydre `confirm:true`, med en særskilt
rammegrænse på 70 MiB + 64 KiB. Andre værktøjers grænser ændres ikke.

`bank-accounts` oplyser profilbeskrivelsen. Den registrerede profil gemmes i et
kildebevis, så senere bogføring og afstemning først kontrollerer den originale
filhash og dernæst genlæser bytes med samme dekoder og datofortolkning.

Group72 består med eksisterende web-UI, afvisning af ugyldige input og adgang,
dubletter, saldoadvarsel, klientparitet, genstart og signeret kildefri
gendannelse under en ny ejer. Group82 kvalificerer desuden oprettelse og
opdatering af åbne bankposters bilagsvejledning gennem import, senere bilag og
bogføring. Dette er gennemgangsopgaver; det erstatter ikke særskilte scorede
forslag til faktura-, købs- og refundmatch.

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

Forløbet understøtter fulde danske DKK-fakturaer med danske CVR/momsidentiteter:
standard 25% moms, blandede momspligtige og fritagne linjer samt dansk omvendt
betalingspligt. Under **Moms på fakturaen** vælges **Momsbehandling**.
**Specificér moms pr. linje** åbner linjernes momstype. Ved omvendt betalingspligt
angives det afklarede danske lovgrundlag og en note; serveren kontrollerer
klassifikationen. PDF og uforanderlige originaler viser den valgte behandling.
Køber afregner momsen ved dansk omvendt betalingspligt; sælger opkræver ingen moms.

Hver klassificeret linje afrundes og bogføres med sit eget grundlag. Eksempel:
100 kr. med 25% moms og 50 kr. fritaget giver 150 kr. netto, 25 kr. moms og
175 kr. til betaling. Kreditnotaer følger originalens konkrete nettobeløb og
momsposter. Delkreditering og slutkreditering bevarer alle oprindelige øre og
konti; historiske fakturaer genberegnes ikke efter de nye linjeregler.
En kredit, der alene består af moms uden et positivt omsætningsgrundlag, afvises
fortsat sikkert og kræver et særskilt korrektionsforløb.

Ugyldige beløb og datoer, nummergenbrug og dobbelt bogføring afvises. PDF
understøtter det nuværende latinske tegnsæt, herunder danske bogstaver; tekst
uden for tegnsættet afvises ved input. Fremmed valuta, udenlandsk omvendt
betalingspligt, OSS og forenklede salgsfakturaer er fortsat restarbejde.
Udstedelse sender ingen faktura eksternt; eksplicit SMTP-afsendelse og offentlig
e-faktura er beskrevet i deres egne afsnit nedenfor.

Det samlede momsforløb er afprøvet på syntetiske data med almindeligt ejer- og
reviewerlogin/MFA: forhåndsvisning, udstedelse, bogføring, delvise og fulde
krediteringer, bankbetaling og to tilbagebetalinger, momsrapport og gennemgået
periodelukning. De 15 journalposter, originale PDF/JSON-filer og klassificerede
kreditfordelinger er verificeret gennem HTTP, CLI og MCP samt genstart og én
kildeuafhængig gendannelse med ny ejer efterfulgt af genstart. Fire signerede
backupøjebliksbilleder er kontrolleret uafhængigt. Installeret desktop og de
resterende fakturavarianter er ikke kvalificeret af denne prøve.

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

Rust-cockpittets startside viser nu porteføljen med datovalg og aktuelle adgangsrettigheder. Likviditet viser historiske bankbevægelser og den eksisterende 13-ugers prognose. Resterende fakturavarianter, installerede desktopforløb og produktionsmigration er fortsat under omskrivning.

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

## Rust-desktop med fælles web-UI

Fra den godkendte beslutning 20. oktober 2026 er standardprogrammet
`rentemester-desktop` en Tauri 2-host med det eksisterende React-web-UI.
Den starter enten den administrerede lokale Rust-server eller forbinder til
et eksplicit valgt HTTPS-serverorigin. Login og MFA foregår i det samme
web-UI inde i appen. Regnskabsregler og adgang forbliver på Rust-serveren.
Der er ingen automatisk lokal reserve ved serverfejl.

```sh
CARGO_PROFILE_DEV_DEBUG=0 python3 scripts/rust/cargo-guard.py cargo +stable build --locked -p rentemester-desktop --bin rentemester-desktop -j1
crates/rentemester-desktop/build-macos.sh
```

Start appen uden flag for at vælge **Åbn lokalt regnskab**, **Opret lokalt
regnskab** eller **Forbind til server**. Udviklings- og acceptkørsler kan bruge
`--local`, `--create-local`, `--local-workspace PATH`, `--server-binary PATH`,
`--web-dir PATH` eller `--server HTTPS_ORIGIN`. Lokal standardmappe er fortsat
`~/Library/Application Support/Rentemester`; acceptprøver skal bruge en
særskilt syntetisk mappe. Menupunktet **Lokal drift** giver adgang til det
private lokale e-mailbekræftelseslink og den offentlige backupnøgle.

Web-UI og API bruger den valgte servers samme origin, også i lokal drift.
Serverindhold får ingen native IPC-rettigheder. Startsidens ene native
kommando kræver den indbyggede startside, dens vinduesidentitet og dens
særskilte capability. Den godkendte grafik fra `assets/branding/` genbruges;
den indbyggede PNG-kopi har identiske RGB-pixels og et uigennemsigtigt
alfalag, som Tauri kræver.

Den installerede usignerede macOS-pakke er nu afprøvet med syntetiske data:
normal første ejer, privat e-mailbekræftelse gennem appmenuen, adgangskode,
TOTP-tilmelding og portefølje, afslutning og genåbning med samme ejer, data
og uændrede konfigurations- og backupnøgler. Begge afslutninger stoppede
den ejede Rust-server. De 12 pakkefiler matcher manifestet. Desktoptests
består med 9 beståede og én eksisterende ignoreret test; kodekontrol og
bygning består. Evidens:
`/tmp/rentemester-rust-reports/shared-web-desktop86-local-onboarding.json`.

Det installerede fakturaforløb er nu gennemført: forhåndsvisning uden
allokering, særskilt udstedelse af faktura `2026-0001` på 200 kr. netto og
50 kr. moms, bogføring, bank-CSV-import og afstemning til nul åbent beløb.
Præcis to journalposter, fem linjer, én banktransaktion og tre dokumenter
er bevaret. Debitor er nul, bogført bank 250 kr., omsætning 200 kr. og
udgående moms 50 kr. Kontoudtogets faktiske saldo er fortsat ukendt, fordi
testfilen ikke indeholder saldi. En separat testkvittering er uploadet via
den native filvælger og vist som PDF uden at bogføre den.

**Åbn PDF** viser originalen; **Hent PDF** åbner på macOS en native
**Gem som**-dialog. Den gemte fil matcher originalens hash præcist.
Den eksplicitte dialog erstatter en implicit Downloads-skrivning, som i
testen blokerede WebKits hovedtråd under macOS-mappeadgang. Annullering
afbryder download; eksisterende filer bevares. WebKit bruger fortsat sin
aktuelle session, og downloads er begrænset til det valgte serverorigin
eller dets egne blob-URL’er. Fakturaens PDF-kopi tilbyder ikke længere
den misvisende udgiftsbogføring; selve Rust-kontrollen afviste allerede dette.

Samme installerede host er afprøvet mod en uafhængig HTTPS-server med
OS-betroet certifikat, normal adgangskode/TOTP, fakturaoriginal og genimport:
nul nye bankposter og én dublet sprunget over. Appens afslutning lod den
uafhængige server fortsætte; genåbning og ny normal MFA viste samme regnskab.
Separat autentificeret API-kontrol med Secure-cookies bekræftede beløb og
begge PDF-hashes. Den administrerede lokale server stoppede ved normal
afslutning. Alle testprocesser er nu lukket, og profilens konfiguration og
private/offentlige backupnøgler er uændrede.

Desktoptests: 9 beståede og én eksisterende ignoreret; 20 berørte bilags-UI-tests,
streng Clippy og web-/native-bygning består. Den endelige installerede,
usignerede udviklingspakke indeholder det fælles web-UI og det godkendte ikon;
alle 12 manifestfiler er verificeret. Evidens:
`/tmp/rentemester-rust-reports/shared-web-desktop88-local-financial-https.json`.
HTTPS-prøven er på samme Mac og kvalificerer forbindelsesgrænsen; den beviser
ikke produktionsdrift på Linux, hele rettigheds-/adgangstabs-matricen, en ny
signeret backup/restore-prøve eller signeret distribution og opdateringer.
Eksisterende TypeScript-workspaces kræver fortsat en egentlig, verificeret
adoptionsvej med kilde-writer-fence før TypeScript-backenden kan udfases.

GPUI-tests,
skærmbilleder, tidligere låst-skærm-observationer og desktopparagraffer
nedenfor beskriver den historiske GPUI-implementation og beviser ikke den
nye hosts login, filupload, download eller PDF-funktion. Den historiske
binær kan bygges særskilt med `--features legacy-gpui --bin rentemester-desktop-gpui`.

## Historisk GPUI-desktop

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
  python3 scripts/rust/cargo-guard.py cargo +stable build --locked -p rentemester-desktop --features legacy-gpui --bin rentemester-desktop-gpui -j1
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
og erstatter kun en eksisterende Rentemester-bundle. Begge Rust-builds bruger
`cargo-guard.py` og én compilerproces. Targetmappen skal være repositoryets fælles
cache; et separat `CARGO_TARGET_DIR` afvises. `CARGO_BUILD_TARGET` og
`RENTEMESTER_DESKTOP_PROFILE` vælger platform og profil. De byggede webfiler
skal være klar før kommandoen køres.
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


## CFO-overblik med sporbare kilder

Det eksisterende `/cfo` bruger Rust-serverens `GET /api/cfo-analytics`. Virksomheds- og porteføljevisninger bevarer eksakte beløbsstrenge, bilags- og posteringsreferencer, den eksplicit koblede part og godkendte dimensioner. Filtre og stabile sider ændrer ikke summerne for hele det valgte datagrundlag. En portefølje summeres kun ved eksplicit valg og fuld adgang; en delvis visning viser ingen totalsum eller skjulte selskabers kilder.

Datoafgrænsning medtager originaler og tilbageførsler på deres faktiske datoer: en senere tilbageførsel fjerner ikke den oprindelige effekt fra et tidligere interval. Koncernvisningen delegerer til den eksisterende godkendte koncernrapport og binder rapportens faktiske selskaber til det valgte omfang. Manglende profil giver en tydelig blokeret visning uden konstruerede tal.

`remote cfo-analytics` og MCP `cfo_analytics` bevarer det fulde native svar uden lokale beregninger. Den faktiske verifikation omfatter to almindelige MFA-brugere, to selskaber, originale købsbilag, dateret tilbageførsel, part/dimensioner, filtre/sider, ændrede rettigheder, web og begge klienter. Genstart og gendannelse under en ny ejer uden kildeworkspace eller gammel signeringsnøgle bevarer de samme kilder og tal. Sammenlignede signerede snapshots viser uændret virksomheds-audit og bogføring.

Det løbende bogføringsgrundlag er kvalificeret i Group61. Group62 kvalificerer nu også originale historiske arkivposter sammen med aktive poster, inklusive genstart og signeret gendannelse. Koncernrapporter bygger på den aktive journal; eksplicit åbningsbalance og efterfølgende godkendt koncernrapportering er kvalificeret i Group64. Der udledes ikke historiske afslutningsdatoer fra arkivets årstal. En godkendt koncernprofil og selve webknappen til paginering er ikke kvalificeret i dette konkrete browserforløb. Der er ingen påstand om færdig desktop- eller samlet migrationsleverance.

## Importér afsluttede regnskabsår

Åbn virksomhedens **Arkiv**. Angiv den historiske basisvaluta og upload de
originale Posteringer.csv-filer for mindst to år; en Saldobalance.csv er valgfri
for hvert år. Gennemgå kildehash, antal poster og år. Det højeste år udelades
som kontrol for overgangen. Bekræft valutaen og importen. Valutaen er din
erklæring; CSV-filerne dokumenterer den ikke selv.

Importen bevarer den nøjagtige kildepakke og hvert kildebeløb med seks
decimaler. Rapportbeløb afrundes særskilt pr. kilderække. Der oprettes ingen
aktive posteringer eller åbningsbalance, og kontoplan og virksomhedsprofil
ændres ikke. Arkivlisten viser årene og giver adgang til deres poster og
originale kildepakke. CFO-overblikket kan medtage arkivposter med sporbar kilde.

Web, HTTP, CLI og alle 191 MCP-tools er kontrolleret sammen med aktuel adgang,
oprindelig nøgles gentagelse, konflikt, genstart samt uafhængigt verificeret
signeret backup efter fjernelse af kildedata og privat nøgle. En ny ejer med
MFA kan gendanne og genåbne de samme arkivdata og originaler. Historisk
import giver ingen aktuel brugerautoritet.

CLI: `remote archive-import-plan` og `remote archive-import-apply --confirm`
læser den nøjagtige native kildeanmodning fra stdin. `remote archives` har
valgfrit `--fiscal-year YYYY`, og `remote archive-import-original --import-id ID`
henter originalbytes. Alle kræver eksplicit server, cookie-fil og virksomhed.
Efter et uafklaret importudfald skal arkiv og kildehash afstemmes, før et
særskilt forsøg med samme nøgle og nøjagtigt samme anmodning.

De almindelige historiske regnskabsvisninger og deres CSV/PDF-eksport er
kvalificeret i Group63. Vælg det konkrete arkivår i årsvælgeren; samme årstal
kan findes både som arkiv og aktiv bogføring. Sidemenu, kontolinks og eksport
bevarer den valgte kilde. Posteringer, saldobalance, resultatopgørelse,
balance og overblik viser den originale arkivkilde. De fire regnskabs- og
posteringsvisninger kan hentes som både CSV og PDF med præcise beløb,
historisk valuta og kildehash.

Saldobalancen bygger på originalens afslutningssaldi. Mangler filen, vises
utilgængelige tal. Ukendte kontoklassifikationer vises med deres kildebeløb;
resultat og balance hævdes ikke komplette, før opsætningen er afklaret.
Arkivår har ingen dokumenterede periodegrænser; der vises ikke opfundne
afslutningsdatoer eller sammenligningstal. Manglende foregående aktive
bogføringsår giver også utilgængelige sammenligningstal frem for nul.

Web, CLI/MCP-paritet, otte eksporter, adgangsafslag, beskadiget fysisk
original, uændret bogføringsaudit, genstart og signeret kildefri gendannelse
under en ny ejer med MFA består. Desktop og den samlede Rust-omskrivning
er fortsat åbne. Eksplicit åbningsbalance er efterfølgende kvalificeret i Group64.

CLI: de eksisterende `remote journal`, `trial-balance`, `income-statement`
og `balance` samt `overview` accepterer `--year` og `--source live|archive`.
`--account` gælder kun posteringer. `remote report-export --report journal`
`--format csv|pdf` bruger de samme kildevalg og returnerer originale
serverbytes; de øvrige rapportnavne er `trial-balance`, `income-statement`
og `balance`. MCP har tilsvarende kildevalg og `report_export`.
Udeladt kilde bruger serverens valg med aktiv bogføring først; en eksplicit
arkivkilde falder aldrig tilbage til aktive tal.


## Eksplicit åbningsbalance og overgang til aktiv bogføring

På virksomhedens administrationsside kan ejeren gennemgå en skæringsdato og
præcise debet-/kreditbeløb og derefter bekræfte én åbningspostering. Eventuelle
arkivreferencer dokumenterer kilderne; arkivsaldi lægges ikke automatisk til
beløbene. Journal, åbningsmarkør, kildebevis og kvittering gemmes atomisk.
Kvitteringen linker til den konkrete postering og virksomhedens faktiske
regnskabsår. Senere bogføring bevares. En ny nøgle opretter ikke endnu en
åbningsbalance, og almindelig tilbageførsel kan ikke neutralisere den.

Aktive virksomhedsrapporter og godkendt koncernbalance efter skæringsdatoen
bygger på den aktive journal. CSV/PDF og web viser, når regnskabsåret kun er
delvist dækket. En koncernrapport før skæringsdatoen afvises; historiske
CFO-kildeanalyser kan stadig læse arkivets faktiske posteringsdatoer.

CLI: `remote opening-balance`, `remote opening-balance-plan` og
`remote opening-balance-apply --confirm` bruger de almindelige server- og
virksomhedsflag. Plan læser input-JSON fra stdin; apply læser den gennemgåede
input/planHash/idempotencyKey/confirm-konvolut. MCP har de tilsvarende tre tools.
Beløb, nøgle og gennemgået plan sendes uændret, og et uklart resultat giver
ingen automatisk genafsendelse.

Group64 består med almindelig MFA, faktisk webbekræftelse, adgangs- og
dubletafslag, klientparitet, genstart og signeret kildefri gendannelse. En ny
ejer skal eksplicit gennemgå og tilknytte den importerede kvittering på
flyttesiden, før den oprindelige nøgle kan genkalde resultatet. Tilknytning og
y genstart giver ingen ekstra åbningspostering. Det oprindelige auditspor
bevares; bevidste kvitteringsgenkald tilføjer kun de forventede auditlinjer.
Installeret desktop og hele Rust-omskrivningen er fortsat åbne.


## Scanning af almindelige bilagsuploads

Serverens private JSON-konfiguration kan kræve scanning ved almindeligt
bilagsupload. Samme indgang bruges fra web, CLI og MCP:

```json
{
  "documentScanner": {
    "policy": "required",
    "provider": "http-json-v1",
    "url": "https://scanner.example.invalid/scan",
    "bearerToken": "<leverandørens private nøgle>",
    "timeoutMs": 15000
  }
}
```

Uden feltet er politikken `off`. `off` sammen med leverandørfelter og
`required` uden gyldig opsætning afvises ved opstart. Timeout er 100–120000 ms.
Udbyderen modtager SHA-256, filnavn, MIME-type og originale bytes som base64 og
skal levere et direkte, ukomprimeret JSON-svar over HTTPS. Et rent svar har
`ok:true`, `scannerId` og eventuelt `scannerVersion` og `evidenceRef`.

Til en privat scanner kan `trustedCaFile` pege på en absolut PEM-sti. Denne
udtrykkelige tillid tillader også en lokal HTTPS-adresse med literal loopback-IP;
certifikat og værtsnavn kontrolleres fortsat. Den almindelige opsætning bruger
serverens offentlige CA-rødder og en adresse uden loopback.

Rettigheder kontrolleres før scanningen og igen før lagring. Andre
forespørgsler kan ændre brugerens adgang undervejs. Original, bilag og
SHA-bundet clean-kontrolspor gemmes samlet efter et godkendt svar og den
afsluttende adgangskontrol. Historiske bilag bevarer deres oprindelige
kontrolspor; leverandørnøgler hører til operatørkonfigurationen.

Indstillingen omfatter det almindelige bilagsupload. Fælles indbakke og
selskabsdokumenter er uden for denne scanningsindstilling. Den lover hverken
OCR eller automatisk regnskabsmæssig godkendelse.

Group65 består med faktisk webupload, betroet lokal TLS-udbyder, afslag og
timeout uden lagrede bilag, offentlig tilbagekaldelse af adgang under en
igangværende scanning, CLI/MCP, genstart og signeret kildefri gendannelse med
ny ejer og MFA. Originaler og kontrolspor bevares uden nye scanninger eller
importerede leverandørnøgler. Den samlede Rust-omskrivning er fortsat åben.


## Læs tekst fra et registreret PDF-bilag

Rust-serveren kan udtrække tekst og sidelayout fra et registreret PDF-bilag
gennem en separat native proces. Upload starter ingen parsing. Operatøren
vælger en pakket worker med den fastlåste PDFium-version og tilhørende
licenser; serveren downloader intet og bruger ingen systeminstalleret læser:

```json
{
  "documentPdfWorker": { "bundleDir": "/absolute/private/pdf-worker-bundle" }
}
```

Mappen indeholder `rentemester-pdf-worker` og dens `pdfium/`-mappe. Versions-
og SHA-256-pins findes i `scripts/rust/pdf-worker/artifacts.json`;
`fetch.py` er kun et byggeværktøj. Ugyldig konfiguration afvises ved opstart.
Uden indstillingen kan gemte resultater stadig læses og gendannes.

En bruger med uploadrettighed kan bekræfte
`POST documents/:id/parse` med `{"confirm":true}` eller behandle højst 100
ventende bilag med `POST documents/parse-pending`. Læserettighed giver adgang
til `GET documents/:id/parse-status` og `GET documents/:id/parsed-text`,
hvor `offset` og `limit` vælger højst 10 sider. Stierne ligger under
`/api/companies/:slug/`. CLI og MCP tilbyder de samme operationer;
`rentemester-rust remote --help` beskriver flagene.

Resultatet indeholder tekst, sidemål og kontrolsummer, men hverken OCR,
fakturafortolkning eller bogføring. Et dokument uden tekstlag får status
`no_text_layer`. En ødelagt PDF får sin egen fejlstatus. Parserens faktiske
identitet gemmes, og eksisterende pdf.js-resultater bevarer deres oprindelige
kontrolsummer. Bogfør bilaget særskilt gennem den eksisterende webformular.

Workerprocessen har begrænset input, output, CPU og køretid. På den afprøvede
Mac overvåger serveren hukommelsen hvert 10 ms og stopper processen over 512 MiB;
der kan være overskridelse mellem målinger. Linux-grænsen er implementeret,
men denne kvalifikation omfatter Mac arm64. Rettigheder og den præcise
original kontrolleres igen, før et resultat gemmes. Der holdes ingen
databasetransaktion eller global ejerlås under parsingen.

Group66 består med faktisk upload, tekst og cache, CLI/MCP, manuel
webbogføring, ændret original, genstart og signeret kildefri gendannelse
med ny ejer efter sletning af kilde, signeringsnøgle og workerbundle.
Gemte resultater kan læses efter gendannelse uden at starte parseren.
SQL-evidenstamper og fravær af processtart ved cache dækkes af fokuserede
tests; de er ikke rapporteret som instrumenterede målinger i brugerprøven.
Installeret desktop og samlet distribution er fortsat åbne.


## Udtræk fakturafelter fra et PDF-bilag

Operatøren kan konfigurere en HTTPS-tjeneste til fakturaudtræk i den private
serverkonfiguration:

```json
{
  "invoiceExtractor": {
    "provider": "http-json-v1",
    "url": "https://extractor.example.invalid/extract",
    "bearerToken": "PRIVATE_OPERATOR_TOKEN",
    "timeoutMs": 15000
  }
}
```

Udelad indstillingen eller brug `{"provider":"disabled"}` for at slå tjenesten
fra. `timeoutMs` er 100–120000 ms. Valgfri `trustedCaFile` er en absolut sti
til en privat CA; TLS-værtsnavnet kontrolleres stadig. Tjenesten modtager
JSON med `sha256` og `pdfBytesBase64` og returnerer `{"fields":[...]}` med
feltværdi, konfidens og kildehenvisning. Input er højst 10 MiB PDF og svaret
højst 1 MiB. Der er ingen automatisk netværksgentagelse.

Bekræftet `POST /api/companies/:slug/documents/:id/extract-invoice` med
`{"confirm":true}` kræver aktuel stamdatarettighed. Læs resultatet med
`GET /api/companies/:slug/documents/:id/invoice-extraction` og almindelig
bilagslæserettighed. Begge svar har `{"ok":true,"extraction":...}`;
et bilag uden udtræk har `extraction:null`. Uden provider afvises eksplicit
udtræk med 503/`EXTRACTION_PROVIDER_UNAVAILABLE`, uden forsøg.

CLI tilbyder `documents extract-invoice` med `--document-id` og `--confirm`,
samt `documents invoice-extraction` til læsning. Brug de sædvanlige
`--server`, `--cookie-file` og `--company`. MCP hedder
`documents_extract_invoice` og `documents_invoice_extraction`; skrivekaldet
kræver `confirm:true`. Den indloggede bruger er aktør.

Gemte resultater indeholder kildehenvisninger og bevarer konflikt- og
fejlhistorik. Et identisk færdigt udtræk genbruges kun med samme kilde,
provider og virksomhedskontekst. Providerfejl gemmes som
`needs_resolution`; adgang kontrolleres igen efter netværkskaldet.
Bogfør særskilt gennem webformularen med eksplicit konto, beløb og momsvalg.

Group70 består med faktisk TLS, cache, konflikter, providerfejl,
adgangstilbagekaldelse under udtræk, manuel webbogføring, HTTP/CLI/MCP,
genstart og signeret kildefri gendannelse under en ny ejer. Gendannelsen
bevarer originaler og udtræk uden provideropsætning eller nye providerkald.
Med provider slået til fortsætter almindelig upload af en ny PDF på højst
10 MiB automatisk til udtræk. Det kræver fortsat aktuel uploadrettighed;
den eksplicitte udtræksoperation kræver stamdatarettighed. Bilaget er allerede
registreret, og dets succesfulde kvittering bevares, hvis det efterfølgende
udtræk fejler eller brugerens adgang tilbagekaldes. Providerfejl efterlader
`needs_resolution`; et tilbagekaldt igangværende forsøg færdiggøres ikke.
Dubletter, deaktiveret provider og andre filtyper starter intet nyt udtræk.
Inbox og selskabsbilag får ingen ny automatik.

Group71 består med almindelige brugerroller, eksisterende upload-UI,
providerfejl, dublet, tilbagekaldelse under netværkskald, manuel bogføring,
HTTP/CLI/MCP, genstart og signeret kildefri gendannelse. De aktuelle roller
har ingen bruger med uploadrettighed alene; den interne adskillelse fra
stamdatarettigheden kontrolleres derfor i en fokuseret test, ikke som en
selvstændig brugerrolle i brugerprøven.

## Åbningsbalance og datakilde i desktopappen

Desktopappen har nu en særskilt **Åbningsbalance**-skærm, også tilgængelig
med ⌘3. Vælg selv overtagelsesdato, konti, debet og kredit samt eventuelle
kildehenvisninger. Appen viser serverens plan til gennemgang, før du kan
bekræfte bogføringen. Kvitteringen kontrolleres mod den registrerede
åbningsbalance og den daterede journalpostering. Luk derefter skærmen og
vælg **Rapporter** for at se regnskabet.

Rapporterne skelner mellem aktive og historiske regnskabsår, også når de
har samme årstal. Præcise decimalstrenge bevares, og manglende perioder,
sammenligningstal og klassifikation vises som utilgængelige. Rapportens
dækning fra overtagelsesdatoen fremgår af serverens oplysninger.

Et uafklaret bogføringsforsøg gemmes privat med den oprindelige anmodning.
Genåbning sender intet automatisk. Genlæs udfaldet, før du eventuelt vælger
en særskilt bekræftet gentagelse med præcis samme anmodning og nøgle.

Group67 er implementeret og bygget; 202 native tests består med én
eksisterende ignoreret test. Den faktiske gennemgang i installeret app
mod både lokal og ekstern server samt genstart er fortsat åben.

## Opret virksomhed og importér historik i desktopappen

Vælg **Opret virksomhed** (⌘4), også når ingen virksomhed er valgt. Angiv
navn, regnskabsårets start og navngivning, momsperiode eller ingen
momsregistrering samt eventuelle bankoplysninger. Gennemgå oplysningerne
før oprettelse. Efter kvittering og genlæsning kan du lukke skærmen; appen
opdaterer virksomhedsvælgeren og vælger den bekræftede virksomhed, hvis du
stadig har adgang. Den nye virksomhed har en tom kontoplan.

Vælg **Importér regnskabshistorik** (⌘5) i den valgte virksomhed. Vælg
originale CSV-filer, angiv år og filtype for hver fil samt valutaen.
Serverens plan viser de historiske år og det seneste år, som udelades.
Gennemgå planen og bekræft importen. Kvittering, arkivregistrering og
originalens hash kontrolleres, før appen viser importen som gennemført.

Opret derefter konti og roller under **Virksomhedsopsætning**, og brug
**Åbningsbalance** og **Rapporter** som beskrevet ovenfor. Importen
udleder hverken kontoplan eller åbningsbeløb.

Et uafklaret oprettelsesforsøg sendes aldrig igen automatisk. En kendt
oprettelseskvittering bevares, hvis efterfølgende læsning fejler. Ved
uafklaret import bevares den oprindelige plan, nøgle og kildehash privat;
genåbning kræver de samme filer og en særskilt gennemgået handling.

Group68 er implementeret og bygget. Den samlede native suite består med
219 tests og én eksisterende ignoreret test; to klienttests består også.
Faktisk betjening med lokal og ekstern server samt genstart er fortsat
uverificeret, fordi computerstyringen aktuelt melder Mac’en låst.

## Årsafslutning og årsrapport i desktopappen

Åbn **Perioder og årsafslutning**, og vælg momsperiode eller regnskabsår.
Regnskabsårets datoer kommer fra serverens aktive regnskabsår, også ved
forskudte år. Gennemgå kontrolpakken, før du særskilt bekræfter lukning.
Genåbning kræver en begrundelse. Registrering af en ekstern indberetning
kræver dens reference; handlingen sender ikke en indberetning.

Åbn **Årsrapport og frister** (⌘6), vælg regnskabsåret og forbered
rapporten. Skærmen viser serverens tal, forudsætninger, kildegrundlag og
forpligtelser. Ukendte frister forbliver ukendte. Noter og ledelsespåtegning
er skabeloner til ejerens eller revisorens gennemgang.

Rust-serveren afviser nu en årsrapport, hvis det ønskede år begynder før
den eksplicitte overtagelsesdato, eller arkiver endnu ikke er overtaget
med en åbningsbalance. Et senere komplet år kan forberedes. Hvis det
foregående år starter før overtagelsesdatoen, er sammenligningstallene
utilgængelige frem for nul eller delvise beløb. Det eksisterende web-UI
forklarer også denne årsag.

## Kontrollér en backup uden kilde eller login

Vælg **Kontrollér backup** (⌘7), også fra velkomstskærmen. Vælg den
eksporterede backup og en separat betroet offentlig nøgle på 32 bytes.
Kontrollér filens hash og nøglens fingeraftryk, og bekræft nøglens
uafhængige oprindelse. Nøglen fra selve arkivet etablerer ikke tillid.

Kontrollen bruger de valgte, fastholdte bytes med det medfølgende eller
eksplicit konfigurerede native serverprogram. Den validerer signaturen,
inventaret, de understøttede schemaer, journalkæder og originalfiler.
Den starter ingen server, gendanner intet og ændrer ikke kildefilerne.
Private midlertidige kopier fjernes efter kontrollen.

Group69 er implementeret med 233 beståede native tests og én eksisterende
ignoreret test, tre klienttests, otte relevante storage-tests og tre
webtests. Desktop, server og web bygger. Backup-wrapperens nye tests
kontrollerer proces- og filgrænser med et testprogram; de udgør ikke en
ny kryptografisk gendannelsesprøve. Faktisk native betjening, lokal og
ekstern genstart samt installeret distribution er fortsat åbne.

## Hent bilagsmail og gennemgå originalen

Rust-serveren har nu Bilagsmail i det eksisterende web-UI. Gem postkassens
`host`, `port`, `username`, `password` og eventuelt `secure` og `mailbox`.
Standarderne er TLS og `INBOX`; `secure:false` bruger klartekst TCP.
Adgangskoden returneres ikke fra status. Et mailalias er lokal konfiguration;
det opretter ikke en postkasse eller et domæne.

En virksomhedsadministrator kan vælge **Gennemgå postkasse nu** og bekræfte
én hentning. Serveren gemmer den præcise EML og understøttede PDF-, PNG- og
JPEG-bilag i den fælles dokumentindbakke. Hent originalen, vælg målvirksomhed,
og gennemgå selv bilagstype, valuta og øvrige oplysninger. Bekræft gennemgangen
og vælg **Fuldfør handoff**. Det opretter et ubogført bilag. Beløb, klassifikation
og bogføring udledes ikke fra mailen; gentagelser opretter ikke ekstra bilag.
Mail uden Message-ID eller brugbare bilag forbliver en original med en
undtagelse. Overstore mails får en tydelig afvisningskvittering uden en
opdigtet original eller filhash.

Automatisk hentning kræver en eksplicit startet proces med
`--mail-worker --mail-service-key-stdin --config PRIVATE.json` ved almindelig
genåbning. En aktuel servicekonto med bilagsadgang leverer sin nøgle via stdin;
gem derefter postkassekonfigurationen som en aktuel administrator, så den
konkrete modtagelsesadgang bindes til konfigurationen og servicekontoen.
Serveren kontrollerer adgang og konfiguration igen efter netværkskaldet.
Der gemmes ingen bruger-cookie eller servicenøgle. Hvis begge worker-typer
startes, deles den ene stdin-nøgle kun i hukommelsen.

Operatørfeltet `imapTransport` kan angive `timeoutMs` fra 100 til 30000
(standard 30000) og en ekstra `trustedCaPem`. Certifikatets værtsnavn og
normal tillid kontrolleres stadig. En hentning er begrænset til 32 mails,
25 MiB pr. mail og 64 MiB samlet. Indbakkebackuppens eksisterende samlede
grænse på 32 MiB gælder fortsat.

Eksisterende native control-schema v30 opgraderes eksplicit med
`--upgrade-mail-schema --config PRIVATE.json`, mens serveren er stoppet.
Kommandoen accepterer kun det komplette v30-schema eller det aktuelle v31;
almindelig opstart foretager ingen schemaopgradering.

CLI og MCP har status, konfiguration, sletning, alias, hentning, kildegennemgang
og originaldownload; se `remote --help` og værktøjernes schemaer. CLI skriver
originaler til en ny privat fil uden overskrivning. MCP returnerer de præcise
bytes; klienterne kontrollerer originalens SHA-256. Backup bevarer originaler,
gennemgang og deduplikering. Postkasseadgangskode og aktiv modtagelsesadgang
følger ikke med ved gendannelse.

Group73 er kvalificeret gennem 28 praktiske trin med en syntetisk TLS-postkasse:
manuelt og via servicekonto, gennemgang i browseren, dubletkontrol, tilbagekaldt
adgang under hentning, HTTP/CLI/MCP, genstart og fire signerede gendannelser
efter fjernelse af kilder og private signeringsnøgler. Originaler, gennemgang,
deduplikering og ét ubogført bilag blev bevaret; der blev oprettet nul
bogføringsposter. Postkasseadgang og automatisk hentning var deaktiveret efter
gendannelse. Prøven omfatter ikke en virkelig ekstern postkasse eller den
installerede desktopapp.

## Send e-faktura til offentlige modtagere

Rust-serverens eksisterende fakturaforløb kan fastholde modtagerens EAN,
markering som offentlig modtager samt køber- og ordrereferencer ved udstedelsen.
Senere ændringer af kunden ændrer ikke den udstedte faktura eller dens rute.
Denne native afsendelsesvariant understøtter DKK og almindelig 25 % moms;
øvrige moms- og valutavarianter er fortsat en åben del af omskrivningen.

Operatøren angiver licensen privat i serverens konfiguration, pr. virksomhed:

```json
{
  "publicEInvoice": {
    "virksomhedens-slug": {
      "apiLicenseKey": "<privat licensnøgle>",
      "environment": "test"
    }
  }
}
```

Miljøet er `test` eller `production`. De normale DigiSense-adresser bruges som
standard. Operatørfelterne `baseUrl`, `trustedCaPem` og `timeoutMs` kan angive
et eksplicit HTTPS-endepunkt, ekstra certifikattillid og en frist på 100–30000 ms.
TLS-værtsnavnet kontrolleres stadig; der følges ingen redirects eller automatiske
netværksforsøg. Licensnøglen vises ikke i web-UI, ledger eller backup.

I **Fakturaer** kan en bruger med aktuel afsendelsesadgang bekræfte tilmelding
af virksomhedens eget CVR og navn. Serveren kontrollerer først licensens
registrerede virksomheder og deltagere. En eksisterende registrering genbruges
kun efter bekræftet juridisk identitet; tvetydige eller modstridende oplysninger
afvises. Et nyt companyKey gemmes før den særskilte udgående
NemHandel-registrering. Historiske lokale registreringer kan attesteres efter
samme leverandørkontrol uden at omskrive deres oprindelige rækker.

**Send e-faktura** bruger den uforanderlige faktura og danner deterministisk
OIOUBL, som leverandøren validerer inden leveringen. Registrerede beløb skal
stemme med linjerne; serveren retter ikke historiske beløb for at få en eksport
til at passe. Den beholder præcisionen i antal og enhedspris.

En varig reservation gemmes før leveringskaldet. En allerede reserveret eller
afsendt faktura sendes ikke igen, heller ikke efter ændret opsætning eller et
uklart netværkssvar. Et accepteret leverandør-id bruges kun til
**Opdatér leveringsstatus**. Også en accepteret, senere afvist levering blokerer
genafsendelse. Netværkskald holder ingen Owner-lås eller SQL-transaktion åben;
aktuel adgang og præcis kilde kontrolleres ved faseskiftene.

CLI og MCP har konfigurationsstatus, historik, bekræftet tilmelding, afsendelse
og statusopdatering. CLI-navnene er `public-invoice-config`,
`public-invoice-submissions`, `register-public-invoice`, `send-public-invoice`
og `refresh-public-invoice-status`. Se `remote --help` for kaldets kontrakt.
Den signerede backup bevarer kilder, registreringsbevis og afsendelseshistorik;
private leverandørindstillinger følger ikke med, og gendannelse sender intet.

Group74 er kvalificeret gennem 25 praktiske trin med almindelige MFA-brugere
og en syntetisk TLS-leverandør. Den eksisterende webformular forhåndsviser og
udsteder fakturaen; eksplicit tilmelding og afsendelse bevarer den frosne
modtager og præcis XML. Accepteret afvisning og uafklaret resultat blokerer
ny afsendelse. Tilbagekaldelse af adgang under validering forhindrer afsendelse.
HTTP, CLI og MCP samt genstart og fire signerede backupprøver med gendannelse
uden den oprindelige datamappe bevarer originaler og historik uden genafsendelse.
Private leverandørnøgler følger ikke med. Der er ikke foretaget en virkelig
ekstern afsendelse; den installerede desktops samlede forløb er fortsat åbent.


## Portefølje og historisk pengestrøm

Porteføljens **Pr. dato** afgrænser de regnskabsmæssige beløb til den valgte dato og hver virksomheds faktiske regnskabsår. Kun aktuelt tilgængelige, aktive virksomheder indgår; skjulte virksomheder bidrager hverken navne, antal eller summer. En ukendt banksaldo bliver ikke nul: den samlede bankværdi er utilgængelig, hvis en synlig virksomhed mangler en dokumenteret saldo. Visningen er ikke en koncernkonsolidering. Opgaver beskriver den aktuelle åbne tilstand for daterede kilder, ikke en rekonstrueret historisk opgavestatus. Moms vises for den aktuelle momsperiode, som indeholder skæringsdatoen, med periode og cutoff synligt.

Likviditet viser faktiske importerede bankbevægelser i tolv kronologiske regnskabsmåneder og bevarer den eksisterende 13-ugers prognose. Regnskabsår vælges med det præcise label, også **2025/26**. Valutaer blandes ikke; udenlandske bevægelser kræver bevaret kompatibelt DKK-beløb og kurs. Manglende vurdering giver utilgængelige beløb, modstridende dokumentation afvises. En arkivkilde uden bankhistorik får ikke opdigtede nulværdier eller saldogrænser.

De kanoniske læseflader er `GET /api/portfolio?asOf=YYYY-MM-DD` og `GET /api/companies/{slug}/cashflow?year=LABEL`, CLI `remote portfolio --as-of DATE` og `remote cashflow --company SLUG --year LABEL`, samt MCP `portfolio` med valgfri `asOf` og `cashflow` med `company` og valgfri `year`. `portfolio` er en ny kontrakt og ikke et alias for TypeScript-værktøjet `portfolio_overview`. Adgang kontrolleres før virksomhedens ledger åbnes og igen før svaret afleveres. Skjulte og ukendte virksomheder giver samme afvisning.

Faktura- og kreditnummerering bruger det kanoniske regnskabsårs-id, eksempelvis **2025-26-0001**, mens regnskabsårsvælgeren viser **2025/26**. Eksisterende originaler ændres ikke; filnavnets sikkerhedskontrol bevares.

Group76 er accepteret med 19 trin gennem normal MFA og eksisterende web-UI på syntetiske data: to virksomheder, tre posteringer, fem bankrækker, datovalg, forskudt regnskabsår, begrænset og tilbagekaldt adgang samt nøjagtig HTTP/CLI/MCP-lighed. Fire uafhængigt verificerede signerede workspace-arkiver og en kildefri gendannelse med ny ejer efterfulgt af genåbning bevarer tabeller, originalbilag og audit-historik. Prøven kvalificerer ikke installerede native finansforløb. Aktuel evidens og resterende arbejde fremgår af `docs/architecture/rust-target/implementation/progress.json`.

## EU B2B-servicefaktura i EUR

Rust-serveren understøtter en faktura i EUR med eksplicit `fxRateToDkk`,
`vatTreatment: "foreign_reverse_charge"` og
`reverseChargeBasis: "EU_B2B_SERVICES"`. Kunden skal have et gyldigt udenlandsk
EU-momsnummer. Fakturalinjerne skal være serviceydelser med nul moms og samme
reverse-charge-grundlag. Den oprindelige valuta og kurs bevares i det uforanderlige
fakturabilag; bogføring og momsgrundlag beregnes i DKK. Webformularens forhåndsvisning,
faktura-PDF og udstedelsesbekræftelse viser originalbeløb og DKK-værdi.

Brugeren starter momsnummerkontrollen med **Verificér EU-momsnummer**.
`POST /api/companies/{slug}/vies/validate` tager `{ "vatOrCvr": "..." }`.
Serverens operatørkonfiguration kan angive VIES-endpoint, betroet CA og timeout;
standardklienten bruger det officielle HTTPS-endpoint. En observation kan bruges i
90 dage med nøjagtigt match på momsnummer og tidspunkt. Udstedelsen fryser den
anvendte observation. Et utilgængeligt svar eller forkert identitet er ikke det
samme som et ugyldigt momsnummer. Aktuelle rettigheder kontrolleres både før opslag
og før lagring; netværkskald holder ingen ledger-transaktion åben.

Delbetalinger og refunderinger bruger bankimportens oprindelige valuta, faktiske
DKK-beløb og kurs. Fakturaens oprindelige DKK-hovedstol fordeles kumulativt, mens
realiserede kursgevinster og -tab bogføres særskilt. Virksomheden vælger eksplicit
rollerne `fx_gain` og `fx_loss`; de er valgfrie for almindelig DKK-drift og gemmes
som en additiv udvidelse med bevaret historik. Krediteringer bevarer den oprindelige
markedskurs og fordeler de oprindelige DKK-beløb uden at miste øre. Deres separate
`journalFxRateToDkk` er en afledt bogføringskurs for det fordelte beløb.

`GET /api/companies/{slug}/eu-sales?from=YYYY-MM-DD&to=YYYY-MM-DD` viser dateret
EU-salg pr. momsnummer med faktura- og kreditnotekilder. Rapporten og momsens rubrik B
bruger DKK-grundlaget; senere krediteringer ændrer ikke en tidligere periodes
rapport. Rapporten foretager ingen indberetning. CLI tilbyder `vies-validate` og
`eu-sales`; MCP tilbyder `vies_validate` med eksplicit bekræftelse og read-only
`eu_sales`. De eksisterende webforløb håndterer faktura, betaling og refundering;
kreditering med særskilt dato bruger i den aktuelle acceptprøve API'et.

Group77 er accepteret med 19 trin gennem normal MFA på syntetiske data: EUR 100
ved kurs 7,40, DKK 740 i oprindeligt salgsgrundlag, del- og slutbetalinger ved andre
kurser samt del- og slutkreditering med refundering. EU-grundlaget er dateret 740,
592 og 0; slutresultatet er DKK 3 i realiseret kursgevinst. Web, HTTP, CLI og MCP
stemmer overens. Fire uafhængigt verificerede signerede arkiver bevarer tabeller,
originalbilag og audit ved kildefri gendannelse med ny ejer og genåbning.
Installerede native finansforløb er fortsat åbne. Evidens og resterende arbejde
fremgår af `docs/architecture/rust-target/implementation/progress.json`.

## Fakturér privatkunder og udsted forenklede fakturaer

En fuld faktura kræver køberens navn og adresse. En almindelig privatkunde behøver
ikke et CVR-nummer. Sælgerens identitet er fortsat obligatorisk, og offentlig
routing samt omvendt betalingspligt beholder deres særskilte krav.

I webformularen vælger **Fakturatype** mellem **Fuld faktura** og
**Forenklet faktura**. Den forenklede formular bruger en beskrivelse og et beløb
inklusive moms. Køber kan udelades helt. Serveren beregner og gemmer nettopris og
indeholdt moms; den fremstiller hverken køberidentitet, antal eller enhedspris.
PDF'en viser typen **Forenklet faktura** og de faktiske beløb.

Det reducerede API-input er fx:

```json
{
  "invoiceType": "simplified",
  "issueDate": "2026-10-20",
  "currency": "DKK",
  "lines": [{"description": "En beskrevet leverance"}],
  "totals": {
    "grossAmount": 100.07,
    "vatRate": 0.25,
    "vatComputationBasis": "VAT_20_OF_GROSS"
  }
}
```

For dette input bliver netto 80,06 kr. og moms 20,01 kr. Momsrapporten anvender de
verificerede originalbeløb og krediteringernes oprindelige fordelinger. Almindelige
posteringers momskontrol er uændret. Denne kildevalidering kontrollerer også
bogføringsretning, konti, oprindelige journalrelationer og eventuelle valutakurser.

Grænsen er 3.000,00 DKK inklusive moms. Udenlandsk valuta kræver en eksplicit
`fxRateToDkk`; grænsen anvendes på den fastlåste DKK-værdi. Forkerte eller
ufuldstændige beløb afvises før nummerreservation. Offentlige modtagere og omvendt
betalingspligt bruger fulde fakturaer. Det reducerede forløb beregner indeholdt
25-procents moms; det er ikke en OSS-fakturatype.

CLI og MCP bruger de eksisterende preview-/issue-kommandoer med dette input.
Preview reserverer intet nummer. Samme input skal være gennemgået før bekræftet
udstedelse. Ukendte udfald kræver opslag i fakturaer, journal og bilag.

Group78 er accepteret med 19 trin gennem normal MFA på syntetiske data: fuld
privatfaktura på 250,00 kr. og forenklet faktura på 100,07 kr., tre betalinger,
tre kreditnotaer og tre refunderinger. Moms er 70,01 kr., derefter 12,01 kr. og
slutteligt nul; alle konti ender i nul. HTTP, CLI og MCP læser samme data og
returnerer samme PDF ved forhåndsvisning af de nye input. Fire signerede arkiver,
genstart og kildefri gendannelse med ny ejer bevarer originaler og historik.
Den samlede prøve dækker DKK. Udenlandsk forenklet fakturas DKK-grænse er dækket af
kernens tests; det er ikke en selvstændig accept af hele dens valutaforløb.
Installerede native finansforløb og øvrige nævnte migreringspunkter er fortsat åbne.

## OSS-grundlag fra manuelt klassificeret salg

Den native momsvisning kan hente et dateret OSS-grundlag. Vælg **OSS fra** og
**OSS til**, og tryk **Vis OSS-rapport**. Rapporten viser det signerede
salgsgrundlag i DKK og antal posteringer, inklusive tilbageførsler. Den beregner
ikke udenlandsk moms, fordeler ikke efter destinationsland eller sats og
indsender ingen OSS-angivelse.

Ved **Ny postering** vælges **Momsklassifikation → OSS-salg til EU-forbruger**
udtrykkeligt. Forløbet kræver DKK, et bilag og en indtægtskonto i kredit.
Klassifikationen vises i bekræftelsen. En almindelig tilbageførsel bevarer den
oprindelige historik; rapportens datointerval bestemmer, hvilke bevægelser der
indgår. OSS-grundlaget bliver ikke dansk salgsmoms eller rubrik B/C.

Samme rapport findes som `GET /api/companies/{slug}/oss-report?from=YYYY-MM-DD&to=YYYY-MM-DD`,
CLI `remote oss-report --company SLUG --from YYYY-MM-DD --to YYYY-MM-DD` og MCP
`vat_oss_report`. CLI kræver eksplicit server og privat cookie-fil. Rapporten
kræver gældende læseadgang, momsregistrering og verificerbare originaler. Ugyldigt
underliggende momsregnskab giver fejl og intet delvist OSS-resultat.

Den samlede prøve på syntetiske data bevarer dansk moms på 25 kr., mens OSS-grundlaget går fra 1.000 kr. til nul efter tilbageførsel. Den oprindelige dato viser fortsat 1.000 kr. HTTP, CLI og MCP, genstart samt kildefri gendannelse med ny ejer består. Fire signerede arkiver bevarer originaler, posteringer og historik.

## Leveringsoplysninger og enhedskoder på fakturaen

Fakturaformularen har valgfri **Levering**: ingen oplysning, en leveringsdato
eller en periode med både fra- og til-dato. Datoerne skal være gyldige,
perioden skal være i rækkefølge, og en enkelt dato kan ikke kombineres med en
periode. Oplysningerne bevares i den udstedte original og vises i PDF'en.

En valgfri enhedskode kan angives for dokumentet og for hver fuld fakturalinje.
Ingen enhed eller mængde opfindes for en reduceret forenklet faktura. API, CLI og
MCP bruger `deliveryDate` eller `deliveryPeriodStart`/`deliveryPeriodEnd`, samt
`unitCode` på dokumentet eller linjen. Forhåndsvisning og udstedelse skal stadig
bruge identiske input; rettelser kræver ny forhåndsvisning.

En almindelig købers valgfrie momsnummer kan være udenlandsk. Det er en
identifikator fra brugeren; det vælger ikke momsbehandling og er ikke en
VIES-validering. Dansk sælger, offentlige modtagere og omvendt betalingspligt
beholder deres øvrige krav.

Group80 består 19 trin på syntetiske data: fuld standardfaktura med udenlandsk
køber, leveringsperiode og enhedskoder samt forenklet faktura med leveringsdato.
Betalinger, kreditnotaer og refunderinger, samme PDF/input gennem HTTP/CLI/MCP,
genstart og kildefri gendannelse med ny ejer bevarer oplysninger og regnskab.
Historiske foreign-reverse-charge- og valutaformater er fortsat åbne migreringspunkter.

## Udenlandske lovgrundlag og historiske valutafakturaer

Ved udenlandsk omvendt betalingspligt kan **Udenlandsk lovgrundlag** vælge
`EU_B2B_SERVICES` eller de eksisterende kildekontrakter
`DK_MOMSLOVEN_§46_STK_1_NR_3`, `EU_MOMSDIREKTIV_ART_196` og
`EU_MOMSDIREKTIV_ART_199`. Grundlaget følger dokumentet og de eksplicitte linjer;
ændringer kræver ny forhåndsvisning. Alle muligheder beholder krav om køberens
betroede VIES-kontrol, note og fastfrosset valutavurdering. De danske grundlag
forbliver særskilte. Funktionen foretager ingen indberetning.

EU-rapporten bruger udstedelsens fastfrosne VIES-oplysninger og kontrolleret
bogførings-/kreditspor. Senere cacheopslag ændrer ikke den historiske
berettigelse. Originalernes hash kontrolleres mod de gemte bytes uden at
omserialisere JSON eller ændre feltorden.

Den historiske læser genkender udenlandske nulmomssnapshots med udeladte
momsfelter og de ældre fastfrosne valutaaggregater uden native versionsmærke
eller DKK-linjeallokeringer. Den verificerer kurs, beløb og sammenhæng og
beregner kun midlertidige regnskabsallokeringer; originaler bliver ikke
omskrevet eller forsynet med opfundne native felter. Historiske råkurser
normaliseres efter den eksisterende kildekontrakt, mens nye native fakturaer
fortsat kræver en eksakt kurs med højst seks decimaler.

Group81 består 19 trin på syntetiske data med alle tre kildegrundlag gennem
webudstedelse og bogføring. EUR-betalinger, kreditnotaer, refunderinger, dateret
EU-salg, HTTP/CLI/MCP, genstart og fire signerede arkiver med kildefri
gendannelse består. Historiske formater er kontrolleret i målrettede kerne- og
lagringstests. Det er **ikke** en accept af faktisk TypeScript-migrering:
der mangler stadig en understøttet operatørvej til at overtage et eksisterende
workspace. Prøvekonverteringer og native backup-gendannelse erstatter ikke den vej.

## Uafstemte bankposter og bilagsgennemgang

Bankimport opretter gennemgangsopgaver for bankposter uden en bogført afstemning.
Et senere almindeligt bilagsupload kan opdatere vejledningen i samme autoriserede
transaktion. Den tidligere opgave afsluttes med et revisionsspor, og en ny opgave
bevarer den nye sammenhæng. Læsning af opgaver ændrer ingen data.

Et ubogført købsbilag er kun et muligt forslag til en udbetaling, når det er det
eneste bilag med samme beløb og valuta. Beløbet beviser ikke sammenhængen. Flere
mulige bilag, anden valuta og indbetalinger giver ikke et automatisk købsmatch.
I webvisningens Bank åbner **Bogfør** på en udbetaling et eksplicit bilagsvalg og
derefter den eksisterende bilagsgennemgang med bankposten valgt. Konti, moms og
bilag skal gennemgås før bogføring. En rigtig bogføring afslutter den tilknyttede
bankopgave; der bogføres aldrig alene på baggrund af opgavevejledningen.

`GET /api/companies/{slug}/exceptions` kræver aktuel læseadgang. Queryfelterne er
`status=open|resolved|all` (standard `open`) og `includeArchived=true|false`
(standard `false`). Gentagne eller ukendte felter afvises. Eksplicit medtagne
arkivopgaver har `archived:true`; oversigter beholder deres normale filtrering.

`POST /api/companies/{slug}/exceptions/{id}/resolve` kræver aktuel
`CompanyReview`-adgang og et JSON-objekt med valgfri `note` på højst 4096 bytes.
Den native body indeholder ingen `confirm` eller caller-valgt actor. Bodygrænsen
er 16 KiB. En allerede afsluttet opgave giver `resolved:false` uden et nyt
beslutningsaudit. Afslutning er kun en gennemgangsbeslutning: den bogfører ikke og
reparerer ikke bankafstemningen. En ny import kan derfor genoprette en opgave for
en stadig ubogført bankpost.

CLI: `remote list-exceptions --company SLUG --status all --include-archived true`
og `remote resolve-exception --company SLUG --id ID --confirm < note.json`, med
eksplicit server og privat cookie-fil som ved andre remote-kommandoer. MCP:
`exceptions_list` med `company`, valgfri `status` og `includeArchived`;
`exception_resolve` med `company`, `id`, valgfri `note` og `confirm:true`.
Klienterne gentager ikke beslutningen automatisk efter et ukendt resultat;
læs opgaven og revisionssporet før en ny handling.

## Matchforslag til bankposter

I **Bank → Matchforslag** vises forslag til kundefakturaer, købsbilag,
tilbagebetaling af kreditnotaer og mulige leverandørrefusioner. Hvert forslag
viser bilag, matchscore og begrundelser samt et link til originalens bilagsvisning.
Scoren er en rangering og kan overstige 1. Forslagene ændrer ingen bogføring,
afstemninger eller opgaver og vælger aldrig et bilag automatisk.

Luk forslagene og brug den eksisterende **Bogfør**-gennemgang til køb og
kundebetalinger. Kunderefusioner afstemmes under **Fakturaer** efter gennemgang af
kreditnotaen. En mulig leverandørrefusion er alene et spor til det oprindelige
køb; forslaget dokumenterer ikke en kreditnota eller et tilgodehavende.

`GET /api/companies/{slug}/bank/match-suggestions` kræver aktuel læseadgang.
Valgfri `bankTransactionId` vælger én uafstemt post. `max` er et positivt heltal,
standard 5, og begrænser forslag pr. bankpost. Svaret indeholder direkte
`ok`, `count`, `rows` og `errors`. Et ukendt eller allerede afstemt id giver
`ok:false`; ugyldige, gentagne og ukendte queryfelter afvises med HTTP 400.

CLI: `remote bank-suggest-matches --company SLUG [--bank-transaction-id ID]
[--max N]`, med eksplicit server og privat cookie-fil. MCP:
`bank_suggest_matches {company,bankTransactionId?,max?}`. Begge bevarer serverens
resultat uden lokal scoring; CLI returnerer exitkode 1 ved `ok:false`.
Tvetydige beløb og manglende identifikation vises som vejledning til manuel
kontrol. Beløbsvejledning for køb kræver fortsat en udbetaling i bilagets valuta.

Group84 består 20 trin på syntetiske data med alle fire forslagstyper, eksplicit
gennemgået køb, kundebetaling og tilbagebetaling. Syv posteringer, seks bankposter,
fire afstemninger, seks forretningsoriginaler og to faktura-PDF’er bevares gennem
genstart og fire signerede arkiver med kildefri gendannelse og genåbning.
Leverandørrefusionen forbliver rådgivning. Installerede desktopforløb og faktisk
TypeScript-overtagelse er fortsat åbne; se implementeringsstatus for evidens.

## Interaktivt CVR-opslag og virksomhedens stamdata

**Kontakter → Tilføj kunde/leverandør → Slå CVR op** bruger nu Rust-serverens
CVR-provider og den samme virksomhedsspecifikke cache som kontaktimporten.
Opslaget udfylder tomme felter og bevarer manuelt udfyldte værdier. Kontakten
bliver først gemt ved en særskilt bekræftelse i formularen.

**Administrér → Hent fra CVR** opdaterer navn, adresse, postnummer, by,
virksomhedsform, branche, status og revisionsfravalg. Svaret viser de ændrede
felter og en eventuel forskel mellem CVR-regnskabsåret og det valgte regnskabsår.
Regnskabsår, betalingsopsætning, bank, kontoroller og bogføring ændres ikke.
Hver vellykket eksplicit synkronisering bevarer et auditspor, også uden feltændringer.

`GET /api/system/cvr-status` kræver en aktuel workspace-session og viser kun
`cvrStatus.configured`. Operatørens CVR-login bliver aldrig sendt til klienten.
`GET /api/companies/{slug}/cvr-lookup?cvr=TEXT` kræver aktuel adgang til eksterne
opslag og returnerer resultatet under `cvr`. Det kan kalde provider og gemme en
cache; det er derfor ikke et skrivefrit læsekald. `POST
/api/companies/{slug}/sync-cvr` bruger virksomhedens eget aktuelle CVR-nummer og
accepterer kun tom body eller `{}`. Ingen af kaldene accepterer caller-valgt
actor, providerendpoint eller legitimationsoplysninger.

Friske snapshots genbruges i 30 dage. Ved manglende login eller providerfejl kan
et ældre snapshot genbruges med det oprindelige `fetchedAt`; cachealderen bliver
ikke nulstillet. Et svar for et andet CVR-nummer afvises uden at gemme det.
Aktuel identitet og adgang kontrolleres igen efter providerkaldet, før data gemmes.

CLI: `remote cvr-status`, `remote cvr-lookup --company SLUG --cvr DK12345678
--confirm` og `remote sync-company-cvr --company SLUG --confirm`, med eksplicit
server og privat cookie-fil. MCP: `cvr_status {}`, `cvr_lookup
{company,cvr,confirm:true}` og `company_sync_cvr {company,confirm:true}`. Klienterne
bevarer det indre resultat, fejl og tidsstempler. Et forretningsresultat med
`ok:false` giver CLI-exitkode 1. Ukendte udfald gentages ikke automatisk.

Group85 består 20 trin på syntetiske data gennem normal MFA, kontaktformular,
virksomhedsopdatering, bekræftede CLI/MCP-handlinger, manglende CVR-login efter
genstart og fire signerede arkiver med kildefri gendannelse og genåbning. To rå
CVR-snapshots, den manuelt rettede kontakt, fire synkroniseringsaudits og
beskyttede regnskabsindstillinger bevares. Ældre cache og rettighedsændringer
under providerkald er dækket af målrettede lagrings- og applikationstests;
prøven foregiver ikke faktisk tidsaldring eller installeret desktopaccept.

## Overtag eksisterende TypeScript-data

`--adopt-legacy` giver et eksplicit forløb med fastholdt review, varig pensionering
og publicering i et nyt workspace. Gruppe 89 har verificeret en faktisk TS-udstedt
og bogført faktura gennem overtagelse, ny ejer/MFA og genstart med uændrede
journalhashes og original PDF. Se [operatørforløb og præcis acceptgrænse](rust-legacy-adoption.md).
Gruppe 90 har nu også kvalificeret fortsat native bogføring, webdownload af
signeret backup med en arkiveret virksomhed samt kildefri gendannelse, ny
ejer/MFA og genstart uden dubleret gammel bogføring. J03 med flere brugere,
adgangstab og usikre svar i den installerede desktop er næste acceptgrænse.
Den fulde Rust-omskrivning er stadig åben.

## Servicekonto i Rust CLI og MCP

En ejer opretter og aktiverer servicekontoen gennem de almindelige administrationsforløb og giver eksplicit adgang til den ønskede virksomhed. Gem engangsnøglen i en privat fil (Unix: `chmod 600`); brug `--service-key-file` i stedet for `--cookie-file`. Rust CLI/MCP understøtter nu serviceidentitet for `me`, virksomheder, journal og regnskabsrapporter samt manuel bogføring og tilbageførsel. Servicekonti kan også læse kontakter, oprette kunder og gennemføre fakturaens forhåndsvisning, udstedelse, særskilte bogføring, liste og oprindelige PDF. Øvrige virksomhedsoperationer og administration kræver fortsat deres eksisterende menneskelige adgang.

```sh
rentemester-rust remote journal --server https://SERVER --service-key-file /private/service.key --company VIRKSOMHED
rentemester-mcp --server https://SERVER --service-key-file /private/service.key
```

Bogføringskommandoerne `post-journal` og `reverse-journal` læser den præcise bekræftede serverpakke fra stdin, inklusive den oprindelige idempotensnøgle. MCP bruger samme pakke i `envelope`. Ingen automatisk gentagelse: ved tabt svar skal original pakke og nøgle bevares til eksplicit afklaring. En ny autoriseret forespørgsel kan forbruge kvote igen, selv når den returnerer en eksisterende bogføringskvittering. Tilbagekaldt nøgle eller virksomhedsadgang giver heller ikke adgang til gamle kvitteringer. Se [servicekontrakten](architecture/rust-target/implementation/service-company/CONTRACT.md) for de separate kvote- og bogføringsudfald.

Servicefakturering bruger de samme rettigheder, kontoroller, regnskabsregler og uforanderlige originaler som brugerforløbet. CLI `create-customer` og `post-invoice` kræver `--confirm`; `preview-invoice` er en læsning, og `issue-invoice` kræver både `--confirm` og `--preview-reviewed` for identisk gennemgået input. MCP bruger `body` og de tilsvarende `confirm`/`previewReviewed`-felter. Disse operationer har ingen generisk idempotensnøgle og gentages aldrig automatisk. Ved ukendt resultat kontrolleres kontakter, fakturaer, journal og originaler før en ny handling.

## Installeret fjernadgang med flere brugere

Den aktuelle delte web/Rust-desktop er nu afprøvet mod en uafhængig, OS-betroet HTTPS-server med to syntetiske virksomheder og almindelig adgangskode/TOTP. Ejeren oprettede virksomhed nummer to og bogførte fra appen; en normalt inviteret og verificeret bogholder bogførte med egen adgang. Virksomheds- og kontoskift under et tilbageholdt, allerede autoriseret journalsvar bevarede den nye visning og dens adgangsgrænse. Tilbagekaldt virksomhedsadgang under ventende læsning og bogføring gav begge403 og uændrede journaler. Eksplicit genetableret adgang gjorde læsning mulig igen.

En særskilt servicekonto bogførte gennem CLI og genfandt samme kvittering gennem MCP uden dublet. De fem regnskabsvisninger stemte mellem ejerens API og bogholder/service i CLI og MCP; den installerede app viste de samme tre nye posteringer. Appens normale afslutning stoppede ikke den eksterne server. Begge testserverforløb blev derefter lukket særskilt; den lokale profilkonfiguration er bevaret. Ny installeret bygning viser nu en dansk rettighedsbesked ved403 i stedet for intern Rust-fejltekst.28 berørte API-tests og produktionsbygningen består.

Evidens: `/tmp/rentemester-rust-reports/installed-remote95.json`. Pakken er fortsat usigneret, og HTTPS-serveren kørte på samme Mac. Hele J03/R50 er ikke færdigt: tabt accepteret bogføringssvar efter fuld lukning/genåbning af appen skal stadig kvalificeres med holdbar afklaring; den nuværende UI-hukommelse bruger sessionStorage og dækker genindlæsning i samme vindue. Installeret flytning, lokal/server-automatik, signeret distribution/opdateringer og fuld backendudfasning er fortsat åbne.


### Manuel bogføring efter mistet kvittering

Browserens manuelle postering sender `input.recoveryRequired: true`. Rust-serveren gemmer da en `manual_post_recovery_pending`-auditfakt i **samme transaktion** som den oprindelige idempotenskvittering og journalpostering. Før endnu en sådan postering kontrolleres uafklarede kvitteringer for den aktuelle menneskelige bruger og ledger. Ældre kvitteringer og eksisterende CLI/MCP-kald får ikke efterfølgende en ny blokering.

`GET /api/companies/{slug}/manual-journal/recovery` kræver aktuel læseadgang og returnerer kun brugerens egne uafklarede kvitteringer. `POST` til samme sti med `{ "confirm": true, "receiptId": 17 }` kræver aktuel bogføringsret og bekræfter kun kvitteringen; det bogfører ikke igen. Den gentagne bekræftelse er idempotent. Journal, oprindelig kvittering og reserveret nøgle bevares. Udløbet kvitteringsindhold giver fortsat en blokering og kræver eksplicit kontrol af journal/revisionsspor.

Webformularen henter kvitteringer, før en ny postering åbnes, viser den oprindelige journalidentitet/hash og kræver eksplicit afklaring. Den almindelige bekræftelsesdialog gemmer desuden sin umiddelbare blokering **før** afsendelse. Et ukendt svar, herunder en kodet 503, frigiver ikke blokeringen. Ingen finansiel handling gentages automatisk. Der gemmes ikke en kopi af beløb, konti eller formularindhold i browserlageret. Genfinding på serveren er uafhængig af WebView-fanen og serverens port.

Group96 er verificeret mod en rigtig Rust-server med normal adgangskode/TOTP: accepteret 200-svar afbrudt, præcis én postering, ny nøgle afvist, rettighedstab og andre brugere/virksomheder afvist, server stoppet/genstartet på ny adresse og samme kvittering genfundet efter normal MFA. Eksplicit gentaget kvitteringsbekræftelse ændrede ikke journalen.Fire lagertests, syv idempotensregressioner, syv service-regressioner og 61 webtests består sammen med streng Clippy samt server-/webbuild. Rapport: `/tmp/rentemester-rust-reports/manual-post-recovery96.json`.

**Historisk afgrænsning for Group96:** Den installerede prøve afventede dengang en ulåst Mac. Group98 har siden verificeret det fulde manuelle forløb med accepteret svar → helt lukket app → genåbning, også gennem den lokale server på en anden origin. Det er beskrevet nedenfor; fuld J03/R50 på tværs af øvrige operationer er fortsat åben.


### Servicekonto: fra faktura til importeret bankbetaling

Servicekontoen understøtter nu `bank_accounts`, `bank` og eksplicit `import_bank` samt `settle_invoice` gennem eksisterende HTTP-, CLI- og MCP-kontrakter. Ejeren opretter fortsat bankkontoen og dens bogføringskonto. Serviceimport kræver aktuel bogføringsret; banklæsning kræver læseret. Bankadministration afvises før kvoteforbrug, og et sådant afslag pensionerer ikke den fælles owner. Indlæsning, kvote og den efterfølgende bogføring bruger den eksisterende kontrol af aktuelle legitimationsoplysninger og virksomhedsrettigheder.

Originale kontoudtogsbytes arkiveres og hashes uændret. Genimport af samme rækker deduplikeres under den valgte bankkonto. En eksplicit fakturaudligning bruger den bogførte fakturas oprindelige debitorkonto, også når ejeren siden har ændret den aktuelle kontomapping. Betalingsjournalen beholder `user:<subject>` og programmet `service-principal`; operationernes revisionsspor bruger `service_account:<subject>`. Eksisterende menneskelige import-, betalings- og refunderingsforløb er bevaret.

Group97 gennemfører den samme serviceudstedte faktura til nul restsaldo: CLI importerede det præcise BOM/CRLF-kontoudtog, MCP genimporterede uden en ny bankpost og udlignede derefter fakturaen én gang. Faktura og kunde har nul restsaldo, bankens dokumenterede og bogførte saldo stemmer, og den oprindelige PDF er uændret. Forkert rolle, bankadministration, tilbagekaldt adgang/nøgle og gentagen udligning afvises. En rigtig servergenstart med normal MFA-login bevarer resultatet. Kontrol efter stop beviser én betaling, én bankpost, de nøjagtige arkiverede bytes og korrekt attribution. Rapport: `/tmp/rentemester-rust-reports/native-service-payment97.json`.

Import og fakturaudligning har fortsat ingen generisk idempotensnøgle eller automatisk gentagelse. Efter et ukendt resultat kontrolleres bank, faktura, journal og oprindelige kilder før en ny tilsigtet handling.

### Installeret app: accepteret bogføring efter fuld lukning

Group98 er udført med den installerede usignerede pakke fra `9f3ede14`. Den normale ejer gennemgik og bekræftede en syntetisk postering. En privat engangsport holdt det rigtige accepterede HTTP200-svar tilbage; hele desktopprocessen blev afsluttet, før svaret blev frigivet. Journalen indeholdt præcis én ny postering. Efter fuld genåbning og normal adgangskode/TOTP viste fjernforbindelsen den samme uafklarede kvittering og blokerede en ny formular.

Den uafhængige server blev derefter stoppet. Samme profil blev åbnet eksklusivt gennem appens lokale Rust-backend på en anden adresse og port. Normal MFA-login genfandt samme kvittering og journalidentitet. Brugeren afklarede kvitteringen eksplicit i appen; det sendte ingen ny bogføring. Appens lokale server lukkede med appen, og den oprindelige profilkonfiguration blev bevaret. Rapport: `/tmp/rentemester-rust-reports/installed-closure-recovery98.json`.

Den efterfølgende rettelse i `a36617cd` bevarer genfinding med den oprindelige nøgle, selv mens en kvittering er uafklaret. Kun en allerede reserveret nøgle for samme bruger, ledger og journaloperation kan gå videre til den normale idempotenskontrol; en ny nøgle er fortsat blokeret. Eksplicit serverbekræftelse frigiver også den tilsvarende umiddelbare blokering i en eksisterende browserfane. Fem lagertests, 62 webtests og det rigtige samlede browser-/server-/klientforløb beviser denne rettelse.

De aktuelle pakker fra `a36617cd` er samlet og kontrolleret mod deres præcise 12-fil-manifester. En ny GUI-smokeprøve af netop denne pakke kunne ikke køres, fordi Mac’en igen var låst. Den tidligere revisionsbundne native prøve bevares; den må ikke beskrives som en ny GUI-prøve af den aktuelle pakke. Installeret flytning, øvrige relevante afbrydelsesforløb, lokal/hostet automation, signeret distribution/opdatering og fuld udfasning af TypeScript-backenden er fortsat åbne.


### Installeret app: flytning fra lokal drift til HTTPS-server (Group99)

Den installerede fælles-web-app er prøvet gennem det samlede administrerede
flytteforløb: frys lokal kilde, gem signeret arkiv i den native dialog,
kontrollér signaturen med den separat betroede offentlige nøgle, klargør et nyt
HTTPS-mål, og opret målets nye ejer med normal mailbekræftelse og MFA. Den nye
ejer tilknytter den bevarede bogføringskvittering; kilden gennemgår målets
præcise udfordring og frigiver kun dette mål; målet aktiveres særskilt.

Prøven bruger en reel accepteret postering med mistet svar og et reelt
uafklaret SMTP-forsøg mod en privat testmodtager. Genkald af den oprindelige
kommando returnerer samme kvittering uden ny bogføring. Én ny eksplicit
postering fra desktopappen giver præcis to posteringer på målet. Efter fuld
app- og servergenstart afviser den flyttede kilde gamle og nye skriveforsøg
samt mailafsendelse; målet er fortsat aktivt. Original PDF/JSON, ledgeridentitet
og oprindelige kvitteringer bevares. Mailen forbliver uafklaret og gensendes
ikke. De midlertidige profiler og processer fra denne prøve er afsluttet og
ryddet op. Den aktuelle a36617cd-pakkes tidligere afventende native smoke-test
er også bestået med normal MFA og fem tidligere posteringer bevaret.

Testværktøjerne tilbyder `run-workspace-transfer.py --native-fixture` til
normal forberedelse af de reelle uafklarede effekter. Herefter udfører den
installerede app selve flyttehandlingerne. `run-desktop-https.py` kan klargøre
et helt nyt mål via `--stage-archive`, `--trusted-backup-key`, `--origin-file`
og `--archive-ready-file`. Origin-filen giver den endelige HTTPS-destination
før frysning; operatøren publicerer først en privat JSON-markør med
`archiveSha256`, når den native download er færdig og hash/signatur er
kontrolleret. Delvise filer udløser ingen staging. Forkert markørhash afvises
før oprettelse af server/ledger. Arkivets signatur kontrolleres fortsat af
Rust-serveren. Native fixtures bevarer fremover profiler til eksplicit
operatøroprydning efter kontrol af eksternt startede processer.

Dette kvalificerer det administrerede installerede J06-forløb. Det afslutter
ikke hele R54: ældre eller fjendtlige kopier, rollback efter ny målaktivitet og
flytning med ikke-tom planlægningshistorik har fortsat særskilte åbne krav.
Signeret distribution, opdateringer og udfasning af TypeScript-backenden er
også fortsat åbne. Acceptbevis: `installed-transfer99.json` med tilhørende
fixture- og markørkontrol i den lokale rapportmappe.

### Servicekonto: scannet købsbilag og kreditorbetaling (Group101)

Servicekontoens bilagsindlæsning følger samme todelte scannerforløb som en
menneskelig bruger: indledende adgang og forbrugsregistrering, scanning uden
Owner-lås eller åben SQL-transaktion, derefter ny kontrol af nøgle,
virksomhedsadgang, ledger og filnamespace før publicering. En intern
engangstilladelse binder afslutningen til den oprindelige nøgle; afslutningen
forbruger ikke endnu et kald. Læseadgang alene giver ingen uploadrettighed.
Originaler og den registrerede scannerevidens er fortsat uforanderlige.

Med eksplicit leverede bilagsfakta kan servicekontoen indlæse originalen,
læse dokumenter/originalbytes, registrere en leverandørfaktura, importere det
præcise kontoudtog og registrere betaling mod den konkrete udgående bankrække.
Betaling følger den oprindeligt bogførte kreditorkonto, selv hvis den aktuelle
kontomapping siden er ændret; den gentager ikke udgift eller købsmoms.
Driftsaudit bruger `service_account:<subject>`, mens journalen bevarer
`user:<subject>` og programmet `service-principal`.

Rust-CLI'ens eksisterende kommandoer `documents`, `document-original`,
`ingest-document`, `expense-booking-options`, `payables`, `register-payable`
og `pay-payable` accepterer nu den eksisterende private service-nøglefil.
MCP bruger de tilsvarende eksisterende tools og samme HTTP-kontrakt. Human
kontoadministration og automatisk fakturaudtræk efter upload får ingen nye
servicebeføjelser; det eksisterende menneskelige udtræksforløb er bevaret.
Servicekøbsforløbet bruger derfor eksplicit faktuel metadata. Fuldt agent- og
provideromfang er fortsat en særskilt afslutningsgate.

Fokuseret verifikation: syv hosttests dækker scanner/publiceringsgrænsen og
serviceforbrug, tre applikationstests dækker servicekreditorer, og to
storagetests dækker menneskelig og servicebaseret del-/fuldbetaling med bevaret
oprindelig kreditorkonto. Streng Clippy og alle tre Rust-binærer er bygget.
Den samlede prøve `run-document-scanner.py --service-purchase` består nu
med påkrævet verificeret TLS-scanner, normal ejer/MFA, faktisk CLI/MCP og
genstart. Ét bilag på 125 DKK giver udgift 100, købsmoms 25, én kreditor og
én udgående bankbetaling; præcis to journalposter og nul åben kreditor.
Ændret kontomapping flytter ikke den oprindelige kreditorkonto. Gentagen
registrering og betaling afvises uden ny effekt. En tilbagekaldelse under
en særskilt igangværende scanning afviser publicering, og genstart bevarer
originalbytes, regnskab, afstemning og den tilbagekaldte adgang uden ny scanning.
Terminalkontrol viser ét dokument, én scannerevidens, én kreditor, én betaling
og én bankrække. Rapport: `native-service-purchase101.json` under den lokale
Rust-rapportmappe. Alle prøvens processer er stoppet og det syntetiske runtime
ryddet op. Dette er server/CLI/MCP-evidens; installeret desktop, kildefri
gendannelse af netop denne fixture og fuldt R13/R45-omfang er ikke kvalificeret
af prøven.

Brugeren har efterfølgende valgt at samle de resterende prøver i den
installerede desktopapp til allersidst. Server-, web-, CLI-, MCP- og automatiske
checks fortsætter løbende uden skærmoplåsning. Group100-profilen er forberedt
med normal MFA og bevaret uden kørende worker; den udgør ikke en bestået
desktopprøve.

### Servicekonto: fakturamail og afklaring af leveringsresultat (Group102)

De eksisterende operationer for leveringsstatus, afsendelse, afklaring og et
særskilt gennemgået nyt forsøg accepterer nu en servicekonto med de aktuelle
virksomhedsrettigheder. Status kræver `CompanyRead`; mutationer kræver
`CompanyExternalSend`. En service med læser- eller bogholderrolle kan ikke
sende. Ejerens eksplicitte tildeling af virksomhedsrollen owner kan give
servicekontoen afsendelsesrettigheden. Servicekontoen kan ikke oprette eller
udvide sine egne rettigheder. Den menneskelige MFA-kontrakt er bevaret.

Afsendelsen bruger den samme immutable faktura/PDF, faste modtager, MIME og
leveringsidentitet som det menneskelige forløb. Et logisk servicekald tæller
forbrug én gang; reservation, ny adgangskontrol før SMTP og efterfølgende
registrering af udfald bruger samme interne tilladelse og genkontrollerer
levende nøgle og rettigheder. Der er ingen SQL-transaktion under SMTP. En
allerede registreret afsendelse returnerer den eksisterende levering uden
transport. Fejl efter en gemt reservation eller forsøgt transport rapporteres
som ukendt udfald; de er aldrig automatisk tilladelse til genafsendelse.
Afklaring registrerer eksplicit ekstern evidens og sender ingen mail. Nyt
forsøg kræver den aktuelle tilstandshash og dokumenteret ikke-accept.

Servicehændelser tilføjer et eksplicit principalfelt og auditaktøren
`service_account:<subject>`. Historiske menneskelige hændelser beholder deres
serialiserede bytes og hashes: det nye felt er fraværende for dem. De
samme valideringer bruges ved læsning og backup/gendannelse. CLI-kommandoerne
`invoice-email-status/send/reconcile/retry` og MCP's tilsvarende tools bruger
den eksisterende private service-nøglefil, præcise bodies og særskilt
bekræftelse. Ingen SMTP-hemmeligheder overføres gennem klienterne.

Fem nye domænetests, fire eksisterende hosttests og storagekontrakttesten
består; de dækker blandt andet én forbrugstælling, gentagelse uden transport,
tilbagekaldelse før transport, tab af rettighed efter transport og eksplicit
afvikling af ukendt udfald. Streng Clippy, alle tre Rust-binærer og helper-typecheck består.
Den samlede prøve `run-invoice-email.py --service-delivery --with-backup`
består ved `ab113e47`: normal ejer/MFA og eksplicit serviceadgang; CLI-afsendelse,
MCP-gentagelse uden transport, SMTP550-afvisning og særskilt gennemgået
MCP-retry. En ordinær nøglerotation afviser den gamle nøgles læsning og replay.
Den faktiske server dræbes efter varig modtagelse af SMTP DATA og før kvittering;
efter genstart forbliver leveringen ukendt, og MCP-replay sender intet. CLI
registrerer derefter eksplicit afklaring med den konkrete køevidens uden mail.

Præcis fire SMTP-forbindelser giver tre modtagne mails med den uændrede gemte
PDF: én accepteret, én afvist og senere accepteret, én med tabt kvittering.
Journalen forbliver tom, og originale JSON/PDF-bytes er uændrede. Én signeret
backup verificeres uafhængigt efter sletning af kilde-workspace og privat
signeringsnøgle. En frisk gendannelse med ny normal ejer/MFA bevarer den
præcise servicehistorik, originaler og ledgeridentitet; SMTP er ukonfigureret,
og afsendelse afvises uden transport. Rapporten er
`native-service-mail102.json`; en separat terminalrapport bekræfter, at de
konkrete testprocesser og fixture er væk. SMTP-hemmeligheden findes ikke i
forretningsdata eller rapport. Tilbagekaldelse før SMTP er domænetest-evidens;
den faktiske prøve roterer nøglen før det ukendte forsøg og beviser ikke
rotation under en igangværende SMTP-handoff. Installeret desktop er fortsat
udskudt til slutaccept, og det samlede Rust-mål er åbent.

### Autentificeret pakkeopdatering uden workspace-adgang (Group103)

[Native opdateringskontrakten](rust-native-updates.md) beskriver den nye
`rentemester-update`-binær, signering med ekstern tillidsnøgle og eksplicit
`verify/status/apply/recover`. Desktop, lokal Rust-server, updater og webfiler
verificeres som én pakke. App og server deler installationslåsen; opdatering
kræver begge lukket. Der er ingen automatisk migration eller rollback.

Den headless Mac-prøve ved `2656ee09` består: ti afvisninger bevarer præcise
installerede bytes; normal opgradering bevarer den gamle pakke; én faktisk
SIGKILL under privat staging efterlader den oprindelige installation kørbar.
Eksplicit recovery kasserer staging og bevarer den eksterne workspace-sentinel.
Dernæst samles, signeres, verificeres og publiceres en pakke med de faktiske
Rust-binærer og det eksisterende web-UI; alle tre binærer køres kun med `--help`.
Rapport: `native-update103.json` og separat terminalkontrol. Testversionerne
er syntetiske identiteter, ikke offentliggjorte produktversioner.

Ti runtime-, ni desktop- og én schema-kontrakttest samt relevante crypto-vektorer,
Clippy og binærbygninger består. Den installerede stable-toolchain afgiver en
ikke-fatal `rust-objcopy`/`libLLVM`-advarsel ved debug-stripping; log og præcise
udførte bytes er bevaret. Der hævdes ingen Apple-signering/notarisering eller
GUI-accept. SIGKILL skete før publication; tilstande efter atomisk exchange er
unit-testet, ikke bevist ved et faktisk process-nedbrud. R52's fulde platform-,
installerede update/offline/uninstall- og data-cutover-accept samt det samlede
Rust-mål er fortsat åbent. Resterende GUI-prøver venter til slutaccept.

### Kontrolleret servernedlukning (Group104)

Rust-serveren håndterer nu SIGTERM og SIGINT. Den stopper HTTP-admission,
lader igangværende svar afsluttes og signalerer stop til begge eksplicit
aktiverede workers. Workers starter ingen ny runde efter observeret stop;
en allerede optaget runde afsluttes og afventes, før processen lukker.
En runde kan indeholde flere operationer. Der indføres ingen automatisk
genafsendelse eller hård timeout, som afbryder en ekstern handling.
Containerens stopfrist skal derfor tillade afslutning af de konfigurerede
netværksbudgetter og en optaget worker-runde. SIGKILL eller overskredet
stopfrist er fortsat et crash og bruger de eksisterende regler om ukendt udfald.

Faktiske macOS-processer beviser, at SIGTERM afslutter en igangværende
16 MiB-download og derefter giver exit 0. En separat privat workspace-prøve
beviser SIGTERM, genåbning og SIGINT med exit 0; den eksisterende kontrol af
konkurrerende ejer og filrettigheder består. To tests af den fælles
worker-driver beviser stop før første runde og afventning af en igangværende
blocking-runde uden næste runde. Alle 40 servertests og streng Clippy består.
Evidens: `server-shutdown104.json` med kilde-, binær- og log-hashes.
Der hævdes ikke en faktisk provideroperation under SIGTERM eller Linux-accept.

R53 er fortsat åben: den eksisterende `Dockerfile` og release-workflow
starter stadig Bun/TypeScript. Næste leverance er en særskilt Rust-kandidat
med bygget web-UI, PDF-worker/PDFium og Linux-biblioteker, dedikeret UID,
fjernede capabilities, no-new-privileges og privat workspace-parent.
Almindelig start skal genåbne; oprettelse, restore og migration skal være
eksplicitte operatørhandlinger. Den nuværende produktionsrute skiftes først
efter de aftalte accept- og releasegates. Resterende desktopprøver er fortsat
samlet til slutaccept efter brugerens ønske.

### Rust-container med faktisk installation og genåbning (Group105)

[Containerkontrakten](rust-container.md) beskriver den særskilte
`Dockerfile.rust`, compose-eksempel og eksplicit oprettelse efterfulgt af
almindelig genåbning. Runtime indeholder Rust-server, web-UI, CLI/MCP og
PDF-worker/PDFium; Bun, Node, Python og Rust-toolchain er ikke installeret.
BuildKit-cachen serialiseres pr. arkitektur og bruger den eksisterende
pladskontrol, også når Cargo genbruger alle artefakter. `target/` sendes
ikke i Docker-konteksten.

Den faktiske arm64 Linux-prøve består ved image-konfigurationsdigest
`sha256:a0a3f8baed2f7de333bc68e319b15cbd088f34d2bf48bb1aa2a579c07606e622`.
Der bruges read-only root, dedikeret UID 10001, umask 077, no-new-privileges og
alle fem capability-sæt nulstillet. Den pakkede PDF-parser læser den præcise
syntetiske tekst og rapporterer de fastlåste bibliotekshashes. Normal web-
ejeroprettelse, bekræftelse og MFA efterfølges af bogføring af 125,50 kr., korrekt
saldobalance, resultat/balance og faktiske CSV/PDF-downloads samt præcist
originalbilag. Kontrolleret Docker-stop giver exit 0. Efter ny containerstart
med samme datavolumen virker normal MFA-login, journal, rapporter, original
og idempotent replay uden dobbeltbogføring. Andet stop giver også exit 0.
Begge egne volumener og containeren er verificeret fjernet.

En ældre testhjælper forventede et opdigtet nul for manglende tidligere år;
den er rettet til den eksisterende null-/`prior_native_year_not_present`-
kontrakt. Produktets regnskabslogik er uændret. Den første fejlede prøve og
den beståede friske prøve bevares separat. Nye rapporter/skærmbilleder har
egen mappe, så tidligere acceptartefakter bevares.

Evidens: `rust-container105b.json` samt separat `rust-container105-provenance.json`
med præcise image-, binær-, kilde- og log-hashes. Koden blev samlet fra
`0b36e959` med de registrerede ændringer, og imaget angiver ærligt dirty-kilde;
leverancen er siden committed. CLI/MCP er her kun prøvet med `--help`.
R53's Rust-specifikke release-/Cargo-/PDF-proveniens, SBOM, uafhængig
reproducerbarhed, HTTPS-proxy/principal-accept og godkendt promotion af
præcis digest er fortsat åbne. Dette er ingen produktionsudgivelse, amd64-
eller installeret desktopaccept. Alle tidligere brugerforløb og det fulde
Rust-mål bevares; resterende desktopprøver venter stadig til slutaccept.


### Rust-pakkens identitet og lokal releasekontrol (Group106)

Den pakkede kandidat bruger nu fælles produkt-/web-/Rustversion 0.2.0.
Den rå byggeidentitet binder faktisk compiler/Cargo/target, schema-policy,
låsefiler, vendorkode, fastlåst PDFium og alle 38 runtimefiler samt separat
entrypoint-hash. En uafhængig læsning af det faktiske image bekræfter inventar,
filrettigheder og 225 unikke MCP-værktøjer fra registry. Native metadataflag
kører uden workspace og afviser blandede kald.

Den faktiske Linux arm64-kandidat
`sha256:3ee8557ea9bb027c0626c2c4035f06759051da355bb1b2f1edfe68bc43581c1b`
består normal ejer/MFA, bogføring, rapporter/PDF, originalbilag, to rene stop
og genåbning/idempotent replay uden dublet. Egne containere og volumener er
fjernet. Kilden er ærligt markeret dirty med base `c92ff38d`; imaget er ingen
ren releasekandidat. Rå identitets SHA256 er
`814151f0619f565f9c1db41d44ec888aba2ff39a7643dace221825396af96855`.
Rapporter: `rust-container106.json` og `native-identity106.json`.

Rust-manifest v2 binder den separate rå identitet og afviser dirty-kilde,
versions-/schema-/PDF-/platformdrift og ukendte felter. Den fælles lokale
Digisense-godkendelseskontrol kræver også identitetsfilen og bevarer kravene
om præcis manifesthash/image/version/commit. Syntetisk positiv godkendelse og
afvisning ved manglende eller ændret identitet, ændret manifest og forkert image
består. Den faktiske dirty-pakke afvises som release. 19 release-tests,
fem producenttests, fire native metadataprocess-tests, streng native Clippy,
afgrænset TypeScript-kontrol og lint består.

R53 er fortsat åben: CI/promovering, Rust-SBOM/supply-chain, dækkende native
interfaceparitet og uafhængig reproducerbarhed mangler integration og accept.
Ingen fuld CLI/MCP-forretningsparitet udledes af kataloget. HTTPS/platform- og
recoveryaccept samt den afsluttende installerede desktopprøve bevares.


### Automatisk Rust-pakkeaccept og konkrete paritetsblokeringer (Group107)

Den nye særskilte Rust CI-prøve bygger fra det testede checkout og udfører
samme normale server-/webforløb som den lokale containeraccept. Den genbrugelige
pakkekontrol læser faktiske image-bytes uden udpakning og sammenholder hele
inventaret, modes, entrypoint og levende binærmetadata. Den består lokalt på
den uændrede arm64-kandidat fra Group106: 38 filer og 225 værktøjer. Den rå
identitet er byte-identisk med Group106. Clean-kravet afviser denne ærligt
dirty-mærkede kandidat uden at skrive bestået rapport. Syv producent-/tar-tests
består, herunder filændring, skrivbare modes, links, traversal og dubletter.
Workflow-YAML er parset. Fjernkørsel på Ubuntu/amd64 er endnu ikke kvalificeret;
workflowet er lokal kode og er ikke publiceret eller kørt herfra.

Evidens: `package107.json`, `packaged-identity107.json`, `rm107-unit.log` og
`rm107-clean-refusal.log`. Den uændrede regnskabsprøve106 bevares; den gentages
ikke alene for at tilføje CI-konfiguration.

En afgrænset kildegennemgang fandt konkrete discovery-/paritetsmangler:
`meta_about`, `agent_capability_search`, `agent_workflow_describe` samt HTTP
`/api/agent-capabilities` og `/api/agent-workflows/:id` mangler. Tidligere
backup-health-audit kræver operationer for health/audit/status/destinations-
placering/fjernverifikation/restore, som det native katalog endnu ikke dækker.
Native workspace-backup og offline restore er særskilte kontrakter og erstatter
ikke disse interfaces ved navneoptælling. Næste paritetsarbejde skal lukke de
konkrete agentbrugerforløb og bevare deres sikkerheds-/retrykontrakter; release-
dækningsgaten må ikke springes over. Fuld R53, øvrige restkrav og den afsluttende
desktopaccept forbliver åbne.


### Agentopslag og faktisk leverandørarbejdsgang (Group108)

Native `meta_about`, `agent_capability_search` og `agent_workflow_describe`
har nu HTTP-/CLI-pendanter: GET `/api/agent-about`, `/api/agent-capabilities`
og `/api/agent-workflows/:id` samt `remote about`, `remote agent-capabilities`
og `remote agent-workflow`. Offentlige metadataopslag åbner ingen ledger og
udfører ingen mutation. De beskriver foreløbig `supplier-purchases` /
`supplier-payable-handling`; `fullCoverage:false` bevares. Andre native funktioner
kan eksistere uden at være dækket af dette finite katalog.

Om-serveren viser native runtime/version, faktisk MCP-toolCount (228 i prøven),
kontraktdokumenter og deterministisk kataloghash. HTTP/CLI har toolCount:null,
fordi de ikke ejer MCP-registry. Regelbundle-, commit- og byggetid tilskrives
ikke opdigtet identitet; metadata angiver utilgængeligheden, og pakket identitet
skal kontrolleres separat. Søgning har maks. 200 tegn, cursor 0–1000000 og limit
1–50 (standard 0/10), AND-match og stabil pagination. Ukendte workflows giver
404, ugyldige/duplikerede queryfelter 400. Uden tilsluttet workspace er workflow
live:false. Ved tilsluttet workspace betyder live kun registrerede HTTP-ruter,
ikke brugerens ret eller samtykke til at udføre dem; aliases skal opløses mod
faktisk MCP-katalog/CLI-hjælp.

Beskrivelsen angiver originalbilag, konti/roller, bekræftet request-body,
forfaldsdato og DKK-bankrække som forudsætninger. Registrering og betaling har
unsafe-read-back, ingen generel keyed replay. Betalte poster læses med status:all;
ukendt udfald kræver canonical kontrol og stop ved uafklaret resultat.

Den faktiske normale ejer/MFA-prøve sammenligner about/søgning/beskrivelse på
HTTP/CLI/MCP, opløser alle aliases mod de faktiske registries, afviser ugyldige
opslag og anonyme regnskabsruter og viser uændret regnskab efter discovery.
Derefter bruger den beskrivelsens MCP-tool til registrering og dens CLI-kommando
til betaling af et scannet originalbilag (100 + 25 moms = 125 DKK). Dubletbilag
og -betaling afvises; oprindelig kreditorkonto, journal/bank/rapporter og original
stemmer. Tilbagekaldelse under scanning forhindrer publicering. Efter genstart
er betalt tilstand/original uændret, tilbagekaldt service giver 401, og der scannes
eller bogføres ikke igen. Den egne syntetiske profil er fjernet.

Evidens: `native-agent-payable108.json`, SHA256
`b5afb09e7fab780d7eb3e161651372fc6e7da26c6696f574addd848d1bd1dded`.
Tre katalogtests, to MCP-katalogtests, en rigtig serverprocesprøve, streng Clippy
og afgrænset TypeScript/lint består. Første serverprocesprøve havde en forkert
relativ webfixture; den er rettet til en selvstændig midlertidig fixture og
består. Dette beviser det beskrevne købsforløb, ikke fuld native discovery,
backupdestinationer, CI-release, installeret GUI eller TypeScript-udfasning.

### Backupspærre ved fælles virksomhedsdata (Group112)

De kontrolbaserede ændringer i virksomhedsviden, selskabsdokumenter, ejerforhold,
parter og koncernopsætning bruger nu samme backupregel som bogføringen.
Alle faktisk berørte virksomheder kontrolleres efter fuld autorisation og før
effekter. Serverens beskyttede virksomhedsforbindelse gendannes efter kontrollen;
fejl ved rollback eller gendannelse pensionerer ejeren. Se
[backupvejledningen](rust-backup-destinations.md) for scope og resterende mail-/
indbakkeintegration. Den afsluttende installerede desktopprøve er fortsat udskudt
til allersidst.

Mailopsætning, alias og nye mailaflæsninger kontrollerer nu backupspærren efter
aktuel autorisation. Indbakkeruting kontrollerer den udtrykkelige modtager;
en tidligere accepteret bilagsoverførsel kan fortsat afsluttes uden dubletter
under en ny lås. Se Group113 i [backupvejledningen](rust-backup-destinations.md).

### Oprindelige fakturakommandoer i Rust (Group148)

`rentemester-rust invoice issue`, `create`, `render`, `post`, `settle-bank`,
`credit-note` og `refund-bank` bruger samme Rust-motor som web-UI'et.
Angiv server, virksomhed og én cookie- eller servicelegitimation som ved de
øvrige serverkommandoer. `issue`, `credit-note`, `settle-bank` og `refund-bank`
læser kildekontraktens JSON med `--input FILE|-`; MCP bruger det strukturerede
`payload`-objekt og `confirm:true`. `render` og `post` accepterer dokument-ID
eller fakturanummer. Rendering verificerer den eksisterende PDF.

Ved `post` skal salgskontoen vælges eksplicit med `--revenue-account`; MCP og
HTTP bruger `revenueAccountNo`. Dette er en bevidst forskel fra den gamle
implementerings faste konto 1000. Kontoen skal være en aktiv indtægtskonto
med normal kreditsaldo. Udeladt valg afvises med vejledning, uden postering.
De oprindelige CLI-kommandoer kræver ikke et ekstra `--confirm`-flag;
MCP og HTTP kræver fortsat `confirm:true`. Serveren validerer leverede beløb,
kundekort, adgang og originale bilag. Ved ukendt udfald skal status og
bogføring afstemmes før et nyt forsøg; klienterne gentager aldrig automatisk.

De øvrige oprindelige fakturafunktioner følges fortsat i epic #672.


### Fakturabetaling uden bankimport og med eksisterende postering (Group149)

`invoice apply-payment --input FILE|-` og MCP `invoice_apply_payment`
bruger samme Rust-server. Input er `invoiceDocumentId` eller `invoiceNumber`,
`paymentDate` og `amount`; valgfrit `bankTransactionId`, `journalEntryId`,
`bankAccountNo`, `receivableAccountNo` og `note`. MCP kræver `confirm:true`.
CLI-kaldet bekræfter handlingen. Alle valg og kontroller afgøres på serveren.

Uden bankrække oprettes en DKK-betalingspostering mod den bekræftede bankrolle
og fakturaens oprindeligt bogførte debitorkonto. Et angivet `journalEntryId`
tilknytter i stedet den præcist kontrollerede eksisterende DKK-postering;
den bogføres ikke igen. Beløb, dato, faktura, banktilknytning, konti og
manglende tidligere anvendelse skal stemme. Bankløse betalinger får ingen
kunstige bankrækker. Valutabetaling kræver en rigtig bankrække og understøtter
ikke et leveret journal-ID.

Lukkede eller indberettede betalingsperioder skal genåbnes gennem det normale
periodeforløb før klassifikation. Dette er en bevidst kontrolforskel fra den
gamle klassifikationsgren. Overbetaling og genbrug af samme postering afvises.
Svaret indeholder `paymentId`, `journalEntryId`, `invoiceDocumentId`,
`invoiceNumber`, `openBalance` og de anvendte regler. Ved ukendt udfald skal
fakturastatus og journal afstemmes før et nyt forsøg; der er ingen automatisk
gentagelse. Bankimport, moms og oprindelige dokumenter ændres ikke ved ren
tilknytning af en eksisterende postering.


### Reparation af en uklassificeret fakturapostering (Group150)

Efter udtrykkelig gennemgang kan operatøren bruge CLI-kommandoen:

```sh
rentemester-rust invoice repair-posting --document-id ID \
  --legacy-journal-entry-id ID --reason 'Begrundelse for reparationen' \
  --revenue-account KONTO --company VIRKSOMHED --server SERVER \
  --cookie-file COOKIEFIL --json
```

`--invoice-number` kan erstatte dokumentvalget. De almindelige transport- og
legitimationsregler gælder; der er ikke tilføjet et MCP-reparationsværktøj.
Salgskontoen vælges eksplicit som ved `invoice post`. Den tidligere faste
konto 1000 genindføres ikke.

Serveren kræver præcis én navngivet, aktiv og uklassificeret gammel postering,
ingen banktilknytning, ingen efterfølgende krav/betalinger/kreditnotaer og ingen
anden anvendelse eller ekstra aktiv fakturapostering. Begge berørte datoer skal
ligge i åbne perioder. I én transaktion oprettes den korrekte bogføring på
fakturadatoen og en præcis modpostering på den gamle posterings oprindelige
dato. Originalbilag og den gamle postering bevares. Hvis et trin afvises,
rulles hele reparationen tilbage. Almindelig bogføring og generisk tilbageførsel
beholder deres beskyttelse mod dobbelt eller løsrevet fakturabogføring.

Svaret identificerer gammel, ny og modposteret journal samt den nye posterings
nummer/hash og anvendte regler. Begrundelse og alle identiteter gemmes i audit.
Gentaget reparation afvises. Ved ukendt udfald skal den kanoniske fakturastatus
og de konkrete journalidentiteter kontrolleres før videre handling.


### Oprindelige rykker-, rente- og kravkommandoer (Group151)

Rust understøtter nu `invoice remind`, `post-reminder`, `compensation`,
`claim-compensation`, `post-compensation`, `interest`, `claim-interest`,
`post-interest`, `interest-correction`, `post-interest-correction` og
`settle-claim-bank`. Brug de almindelige forbindelsesflag og vælg faktura med
`--document-id` eller `--invoice-number`. Kravbetaling læser det oprindelige
JSON-payload fra `--input FILE|-`, med faktura- og bank-ID eller præcis reference.

Beregninger er læsninger. Registrering opretter et krav; bogføring er et særskilt
kald. Udeladt rykkergebyr betyder 100 DKK. Udeladt rykker-ID vælger den tidligste
registrerede rykker, også hvis den allerede er bogført; i så fald afvises dublet.
CLI-renter uden `--reference-rate` bruger serverens lovbestemte rentetabel.
De originale flade MCP-rentekald kræver fortsat et eksplicit `referenceRate`.

Ved bogføring angives `--receivable-account` og den relevante
`--reminder-income-account`, `--compensation-income-account` eller
`--interest-income-account`. Der vælges ingen skjult kontoplan. Rentekorrektion
kræver `--reason`; serveren beregner beløb, oprindelige konti og tilladt dato.
Udeladt bogføringsdato følger rykkeren, kravet eller korrektionens kausale
slutdato. Det er ikke automatisk dagens dato.

MCP bruger de oprindelige flade argumenter og `confirm:true` ved ændringer.
De otte eksisterende værktøjer med `body:string` bevarer deres input og svar;
inputformerne må ikke blandes. CLI-kaldet bekræfter som de øvrige originale
fakturakommandoer. Alle handlinger bruger serverens aktuelle rettigheder,
originalkontrol og fælles regnskabsmotor; klienterne genforsøger ikke automatisk.

Hovedstolen skal være betalt, før en faktisk DKK-bankindbetaling kan afvikle
krav. Betalingen anvendes på de historisk bogførte kravkonti. Valutahovedstol
og DKK-kompensation holdes adskilt; kompensationsberegningen angiver
`principalCurrency` og `compensationAmountDkk`. Valuta-renter afvises fortsat
med `unsupported_foreign_interest`, uden regnskabseffekter. Disse kendte
forretningsafvisninger bevarer den maskinlæsbare kode efter sikker rollback;
ukendt udfald kræver fortsat afstemning før et nyt forsøg.

## Oprindelige finansielle fakturakommandoer (Group152)

`invoice validate --input FILE|- [--json]` bruger den fælles Rust-motors rene
validator uden virksomhed, server, PDF eller nummerallokering. MCP
`invoice_validate {payload}` bruger samme funktion og returnerer det oprindelige
valideringssvar i `data`. Ugyldig faktura giver CLI-exit1; brugsfejl giver2.

`invoice write-off-bad-debt --input FILE|-` og MCP
`invoice_write_off_bad_debt {company,payload,confirm:true}` bruger den eksisterende
native tabsføring. Angiv faktura-id eller nummer, `writeOffDate` og en eksplicit
`expenseAccountNo`; beløb kan udelades for resthovedstolen. Ved begge selectors
vinder id. Debitor-/momskontoassertioner kontrolleres mod originalposteringen.
Ændret momskontorolle afvises under den eksisterende native schema-kontrol.
Den tidligere MCP-form `{company,body:string,confirm:true}` bevares som en
adskilt variant; blandede former afvises. Serviceaudit bindes til autentificeret
identitet og journal; menneskelige posteringers eksisterende format bevares.

`invoice imported-receivables --company SLUG [--as-of DATE]` bruger serverens
UTC-dato ved udeladelse. CLI returnerer den direkte oversigt med `ok:true`.
MCP beholder `company/asOf` og tilføjer oversigtens felter direkte under `data`,
samtidig med at native `data.ok` og `data.importedReceivables` bevares. Dette er
en dokumenteret additiv udvidelse; HTTP-resultatet ændres ikke. Importerede
fakturaer forbliver adskilt fra native fakturaer, også når de er betalt.

Group152 er accepteret med faktisk delvis/fuld tabsføring, global audit/moms,
uændrede originaler og genstart. Offentlig e-fakturering og det samlede
omskrivningsmål er fortsat åbne; installeret desktopaccept udføres til sidst.

## Oprindelige offentlige fakturakommandoer (Group153)

`invoice export-public --out FILE` og `invoice export-public-oioubl --out FILE`
eksporterer henholdsvis det særskilte preview-XML og OIOUBL fra den uforanderlige
udstedte faktura. Vælg `--document-id` eller `--invoice-number`; id vinder ved
begge. Angiv server, virksomhed og privat cookie som ved øvrige originale
kommandoer. Filer skrives lokalt efter verificeret XML-hash; serveren modtager
ingen filsti. Eksport kræver aktuelle dokumentrettigheder, ingen udbyder.
OIOUBL-eksporten auditeres. Den fælles eksportør understøtter blandet/0/fritaget
og reverse-charge-moms samt fakturavaluta uden at kræve uvedkommende FX-felter.
Eksisterende DKK-XML og præcise enhedspriser bevares; summer repareres ikke.

`invoice submit-public-peppol --access-point FILE [--out FILE]` bruger filens
`accessPointId`, `endpointUrl`, `senderEndpointId` og eventuelle
`acknowledgement: {transmissionId,acknowledgedAt}`. Endpoint er kun metadata.
Forberedelsen og den udtrykkelige manuelle kvittering kræver frisk menneskelig
MFA med ExternalSend. Samme nøgle returnerer den gemte konvolut; en senere
acknowledgement opgraderer ikke en eksisterende forberedelse. Manuel kvittering
spærrer for afsendelse, men bliver aldrig en native udbyderkvittering.
En verificeret ren forberedelse spærrer ikke den senere native levering.

`invoice transmit-digisense --confirm yes [--digisense-company-key KEY]`
bruger serverens eksisterende registrering, nøgler og udbyderfaser. Det valgfrie
KEY er kun en assertion mod den konfigurerede identitet. Første afsendelse
returnerer `duplicate:false`; gentagne originale kald bruger den bevarede
leveringsidentitet. Kølagt levering kan observeres én gang pr. eksplicit kald,
uden ny levering; bekræftet levering returneres fra historikken. Accepteret fejl,
tvetydigt udfald og manuelle/historiske kvitteringer kan ikke omgås ved nye
access-point-oplysninger. Ingen automatiske retries eller polling-løkker.
Native svarformat og eksisterende statuskommandoer bevares. De fire oprindelige
kommandoer er CLI-only; der er ikke opfundet nye MCP-navne.

Group153 er accepteret gennem normal MFA, faktisk syntetisk TLS-levering,
originale klienter og fire uafhængigt verificerede signerede snapshots med
source-free restore og genåbning. Det samlede omskrivningsmål er fortsat åbent.


## Oprindelige tilbagevendende fakturaer (Group154)

De oprindelige `recurring-invoice create --input FILE|-`, `generate --template-id N --as-of YYYY-MM-DD` og `list [--include-inactive]` bruger samme Rust-server som web-UI'et. Vælg eksplicit `--server ORIGIN --company SLUG` og én privat `--cookie-file` eller `--service-key-file`; `--json` giver det flade core-resultat. Oprettelse og generering bekræftes ved selve CLI-kaldet. MCP-navnene er `recurring_invoice_create`, `recurring_invoice_generate` og `recurring_invoice_list`; MCP-skrivninger kræver `confirm:true` og bruger de oprindelige envelopes med ens text/structuredContent.

Oprettelsesinput er det oprindelige strukturerede `invoice` med parter, linjer, totalsummer og momsbehandling samt `name`, `interval`, `firstIssueDate` og valgfrie `intervalCount`, `deliveryChannel`, `paymentTermsDays`, `deliveryPeriodMode`, `notes`. Defaults er 1/manual/30/issue_month. Serveren fryser specifikationen uden afledte datoer eller fakturanummer. `list` viser aktive skabeloner som standard og bevarer inaktive med flaget.

HTTP-kontrakten er GET `recurring-invoices/original/list?includeInactive=false|true` og POST `recurring-invoices/original/create|generate` under `/api/companies/:slug/`. Generate bruger `templateId`, `asOfDate`, `confirm:true`. Aktuel CompanyRead/CompanyDraftWrite og øvrige eksisterende guards gælder; actor følger det autentificerede menneske eller den typede serviceprincipal. Ingen actor i payload giver adgang.

Samme skabelon/periode refererer til samme faktura og originale JSON/PDF. Den oprindelige `interval_window` slutter på næste beregnede periodegrænse; native skabeloners eksisterende kalenderkontrakt bevares. Web-UI'ets native genereringskald bruger også den frosne oprindelige specifikation, så næste periode bevarer blandet moms og parter. Kanalerne manual/email/digisense er metadata her: create/generate hverken sender, bogfører eller starter en scheduler.

Den aktuelle native udstedelse afviser fortsat andre taxable momssatser end25% og omvendt betalingspligt uden eksplicit note; den gamle source-validator accepterer mere. Det er åbne original/native-paritetskrav, ikke en påstand om fuld paritet. Øvrige domæneafvisninger følger de eksisterende native fejl-/transportkontroller, indtil de resterende oprindelige fejlkontrakter er afstemt. Workspace-kørslen med cap/continuation, isolerede virksomhedsfejl, eksplicit afsendelse og nonempty recovery er næste obligatoriske del.

Accepten på syntetiske data gennem faktiske CLI/MCP/HTTP-processer dækker tre skabeloner, fire generationer inklusive næste periode via web-API'et, otte originale filer, identiske læsninger og genstart. De eksisterende33 posteringer/80 linjer, bankdata og tidligere originaler bevares; ingen afsendelses-events tilføjes. Aktuel bruger-/servicerevokation dækkes af hosttest. Denne accept omfatter hverken en ny Fresh MFA-session, signeret gendannelse af workspace-afsendelser eller manuel installeret desktopaccept.

## Eksplicit workspace-kørsel af gentagne fakturaer (Group155)

`recurring-invoice run-workspace --workspace ASSERTION --as-of YYYY-MM-DD --max-generations 2 --confirm yes --server ORIGIN --cookie-file FILE --json` bruger POST `/api/recurring-invoices/run-workspace`. MCP-navnet er `recurring_invoice_run_workspace` med `{workspace,asOfDate,maxGenerations?,confirm:true}`. Maksimum er 1–5000, standard500 pr. virksomhed; generering og leveringskandidater har hver sin grænse. Datoen må ikke ligge efter serverens aktuelle dato. Workspace er serverens konfigurerede identitet eller præcise absolutte rod som assertion; serveren åbner aldrig en klientvalgt mappe.

Kørslen kræver en aktuel menneskelig workspace-owner-cookie. Servicecredentials understøttes ikke på denne samlede operation. Virksomheder og deres aktuelle rettigheder kontrolleres gennem samme Rust-ejer som øvrige operationer. Kendt fejl i én virksomhed bevarer delresultatet og tillader næste virksomhed; ukendt commit/ejerudfald stopper nye effekter og kræver afstemning. CLI-exit1 kan derfor indeholde allerede udførte generationer. Læs altid `companies`, `errors`, `hasMore`, `remainingGenerations` og `continuation`, før en eksplicit fortsættelse vælges. HTTP bevarer afsendte delresultater med200; indledende adgangs-/inputafvisninger bruger fortsat fejlstatus. MCP bevarer hele core-resultatet under `data` ved succes og delresultat samt ydre `errors` ved forretningsfejl. Ingen timer eller automatisk genforsøg startes.

Den lokale operatør kan sætte `recurringEmailDryRun: {SLUG: {host,port,fromAddress,fromName}}`. Dette validerer modtager, konfiguration og den gemte PDF gennem samme MIME-builder; det sender ingen mail og opretter ingen SMTP-kvittering. En `acknowledged` recurring-event beskriver udtrykkeligt prøvekørslen. Konfigureret almindelig SMTP er ikke automatisk tilladelse til live tilbagevendende e-mailafsendelse. Offentlig levering genbruger den eksisterende DigiSense-konfiguration, registrering, friske MFA og fasede afsendelse. Lokal adgang/registrering/original kontrolleres før reservation; transport udføres uden ejerlåsen. Et accepteret ventende provider-id kan kun observeres, mens uafklaret/terminal/allerede forsøgt levering kræver afstemning og aldrig afsendes igen.

Alle nye delivery-events bindes til generation, original hash, autentificeret actor og leveringsmode gennem en kanonisk auditpost. Signeret backup validerer denne sammenhæng. Historiske events bliver ikke nye tilladelser ved gendannelse.

Group155 er accepteret for den beskrevne menneskelige workspace-kørsel. Faktiske CLI/MCP/HTTP-processer har verificeret fejlisolering mellem virksomheder, cap2/fortsættelse, seks generationer, tolv originale filer, én e-mail-prøvekørsel uden SMTP og identisk tilstand efter genstart. En separat profil med normal MFA og lokal TLS-testudbyder har verificeret to offentlige generationer: manglende tilmelding giver preflight-afvisninger uden reservation; efter eksplicit tilmelding sendes hver faktura én gang. Et ventende ID bekræftes med ét GET, og det tvetydige udfald genafsendes aldrig, heller ikke efter gendannelse. Syv typede recurring-events og hele SQL-/originalhistorikken bevares i fire uafhængigt verificerede signerede snapshots gennem source-free gendannelse med ny ejer og ny MFA-genåbning. Kilde-/målmapper og private signeringsnøgler er fjernet før sidste uafhængige kontrol; operatørcredentials genbruges ikke. Arkiveret/uinitaliseret klassifikation er verificeret i det faktisk åbnede host-katalog uden filoprettelse; serverens skip-mapping er kontrolleret i koden. Aktuel medlemsrevokation og backupspærring før plan/generering/reservation/events er dækket af målrettede hosttests. Dette er ikke en påstand om browsertest af ændrede manifests, service-workspace-paritet eller ny live SMTP-automatik. Hele omskrivningen, resterende oprindelige kontraktforskelle og sidste installerede desktopaccept er åbne.


## Oprindelige momssatser og valgfri reverse-note (Group156)

Fulde originale fakturaer bruger linjens momssats og ellers `totals.vatRate` som
fallback. Udstedelse, gemte JSON/PDF-originaler og tilbagevendende generering
bevarer de faktiske beløb, også ved andre positive eller blandede satser.
En eksplicit reverse-charge-basis kræver fortsat korrekt klassifikation og
nulmoms, men den valgfrie fritekst kan udelades.

En momssats uden understøttet bogføringsklassifikation afvises før bogføring
eller kreditnota med `unsupported_invoice_vat_classification`. Den oprindelige
CLI bevarer den validerede serverfejl og exit1; MCP giver samme forretningsafvisning.
Ugyldige svar bliver ikke til succes, og408/5xx/afbrudte skriveresultater kræver
fortsat afstemning frem for automatisk gentagelse.

Det faktiske forløb har verificeret to skabeloner, fire generationer og otte
originale filer gennem kilde-CLI/MCP og næste periode via web-API.20%-fakturaen
bevarer total120; fakturaen med omvendt betalingspligt bevarer total100 uden
fritekst. Kun den sidste bogføres: én journal med to linjer. Alle tidligere
posteringer, bankdata og originaler samt det samlede resultat er uændrede efter
genstart. En indledende CLI-fejl i fejlkoden blev rettet, hvorefter prøven
fortsatte fra de eksisterende dokumenter uden ny generering. Ny MFA-fornyelse
og signeret gendannelse er ikke påstået for denne prøve. Hele epic'en og den
sidste installerede desktopaccept er fortsat åbne.

## Oprindelige fakturaafvisninger og recovery (Group157)

De eksisterende originale klientforløb bevarer nu en gyldig HTTP422-afvisnings
præcise serverkode og fejl: fire fakturalæsninger, elleve kravoperationer,
gentagne fakturaers create/generate/list og fire offentlige fakturaoperationer.
CLI returnerer exit1; eksisterende originale MCP-tools returnerer isError:true.
Recurring-MCP bevarer serverkoden før tekstbaseret kompatibilitetsfortolkning
af ældre HTTP200-afvisninger uden kode. Der tilføjes ingen nye MCP-tools.

Kun komplette, størrelsesbegrænsede ok:false-svar med ikke-tom kode og gyldige
fejl accepteres som HTTP422-forretningsresultat. Ugyldige svar og adgangsfejl
forbliver generiske. Bekræftelse, læse-/skrivepolitik, usikre skriveresultater
uden automatisk genforsøg samt bevaring af eksisterende eksportfiler er uændrede.

De fire isolerede SGD-leverancer og deres integrerede resultat bestod de
eksisterende fokuserede klient-, CLI- og MCP-prøver samt strict Clippy.
Dette er transportaccept med lokale HTTP-fixtures og faktiske CLI/MCP-processer;
ingen fakturering, levering eller regnskabshandling fra Group156 er gentaget.
Global argument-/miljø-/virksomhedsvalg, standardlauncher, øvrige åbne
originalkontrakter, release/cutover, TypeScript-udfasning og endelig installeret
desktopaccept er fortsat åbne. Evidens: progress.json → original_invoice_refusals157.

## Oprindeligt CLI-kald og virksomhedsvalg (Group158)

De allerede migrerede originale kommandofamilier accepterer globale flag før,
mellem og efter kommandoord samt `--flag=værdi`. Kun det første lighedstegn
opdeler flaget; en tilknyttet tom værdi eller værdi med indledende bindestreger
forbliver data til familiens egen validering. `--format json|human` vælger det
eksisterende outputformat. Oprindelige `find --`-søgeord forbliver bogstavelige.
Familiens tilladte flag, typer, bekræftelser og afvisninger gælder fortsat.

Et eksplicit, trimmet virksomhedsvalg har forrang for `RENTEMESTER_COMPANY`.
Der vælges aldrig automatisk første virksomhed. Remote bruger fortsat slug.
Med `--local-profile ABS_PRIVATE_PROFILE` kan et eksisterende kanonisk
virksomhedskatalog direkte under profilens verificerede workspace omsættes
til slug. Traversering, symlinks, kataloger udenfor workspacet og en modstridende
`RENTEMESTER_WORKSPACE` afvises. Serverens normale cookie eller serviceprincipal
bestemmer identiteten; actor-miljøvariabler giver ingen ekstra rettigheder.
Der åbnes ingen lokal ledger som fallback.

Tomt kald, root-help og selvstændig `--version` læser ikke profil, credentials,
stdin eller netværk. Versionen kommer fra Rust-pakken; ukendte root-kald giver
exit2 med brugbar hjælp. Eksisterende native/eksperimentelle dialekter bevares.

Det integrerede resultat har bestået214 CLI-tests og strict Clippy. En påvist
macOS-race i en eksisterende HTTP-testfixture blev rettet ved at sætte den
accepterede socket til blocking før dens eksisterende timeout. Assertions og
produktkode blev ikke ændret af den rettelse. Dette accepterer de beskrevne
kaldsformer for allerede migrerede familier; alle265 oprindelige CLI-nøgler,
oprindelige automatiske outputdefaults og skift af `bin/rentemester` er endnu
ikke accepteret. Hele epic'en og sidste installerede desktopaccept er åbne.
Evidens: progress.json → original_cli_entry158.
