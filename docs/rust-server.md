# Kør Rust-serveren med web-UI'et

Rust-serveren leverer det eksisterende React-cockpit og behandler selv login,
ejeroprettelse, MFA, sessions, virksomhedsadgang, manuel bogføring, tilbageførsel,
virksomhedsstamdata, kontooprettelse, bekræftelse af kontoroller, bankkonti, generisk bankimport, bilagsbogføring med valutakøb samt bilagsindlæsning og resultatopgørelse, balance og saldobalance med CSV/PDF-eksport samt momsoverblik med PDF, kundeoprettelse og udstedelse/bogføring af danske salgsfakturaer. Den
starter ikke TypeScript-backenden og sender ikke forespørgsler videre til den.

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
   dato;tekst;beløb;valuta
   2026-10-18;Kontorartikler;-250,00;DKK
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

Når CSV'en ikke indeholder en understøttet saldo, viser banken en ukendt
kontoudtogssaldo. Den bogførte saldo beregnes fra ledgeren inklusive
modposteringer fra tilbageførsler; den forveksles ikke med kontoudtogets saldo.

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
Periodelukning og indberetning er endnu ikke tilgængelige i Rust-cockpittet.

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
Kreditnota, fremmed valuta, andre momstyper, offentlig e-faktura,
ekstern maillevering og redigering/sletning af kontakter mangler stadig i Rust.
Der sendes ingen fakturaer eksternt i dette forløb.

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
Fakturaer med kreditnota eller tab på debitorer kræver deres endnu ikke porterede
afregningsforløb. Beløb, der afviger mellem fakturaens rest og ledgerens
tilgodehavende, må ikke skjules ved at markere fakturaen betalt.

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
bekræftelsesdialog og kontrollerer den delvise og fulde betaling. Efter genstart
kontrolleres nul åbent tilgodehavende, to rene bank-/debitorposteringer, seks
afstemte bankposter og uændret salg, moms og resultat. Skærmbilledet gemmes som
`/tmp/rentemester-rust-settlement.png`.
Testdata fjernes fra Docker efter kørslen.

## Aktuel afgrænsning

Launcheren opretter workspacets filer lokalt. Første ejer oprettes i browseren
via et privat installationslink, som den betroede lokale `setupActor` aktiverer
fra serverkonfigurationen. Linket gælder en time og lukkes efter vellykket
opsætning. Servergenstart kontrollerer den gemte status og genåbner ikke en
afsluttet opsætning. Bekræftelseslinks leveres som private lokale filer, ikke
som eksterne e-mails.

Workspace-ejerskabet bruger foreløbig den eksisterende beskyttede Linux-profil;
på macOS kører demoen derfor i Docker. Serverens `--config` bruges med native
workspaces og en betroet lokal konfiguration; `--create` opretter et nyt workspace,
og uden flaget genåbnes det eksisterende. `--origin` angiver browserens lokale
origin, når serveren kører bag Docker-portmapping.

Rust-cockpittets startside viser foreløbig virksomhedsvælgeren. Resten af
porteføljeoverblikket, øvrige bogførings-, rapport- og eksportforløb, desktop og produktionsdrift
er fortsat under omskrivning. Denne demo er ikke den færdige Rust-migration.
