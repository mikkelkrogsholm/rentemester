# Kør Rust-serveren med web-UI'et

Rust-serveren leverer det eksisterende React-cockpit og behandler selv login,
ejeroprettelse, MFA, sessions, virksomhedsadgang, manuel bogføring, tilbageførsel,
virksomhedsstamdata, kontooprettelse, bekræftelse af kontoroller, bankkonti, generisk bankimport, bilagsbogføring med valutakøb samt bilagsindlæsning og resultatopgørelse, balance og saldobalance med CSV/PDF-eksport samt momsoverblik med PDF, kundeoprettelse, udstedelse/bogføring af danske salgsfakturaer samt betalinger, kreditnotaer, afstemning af kundetilbagebetalinger og leverandørkort med bilagsudfyldning samt leverandørgæld med senere bankbetaling. Den
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
afsluttet opsætning. Bekræftelseslinks leveres som private lokale filer, ikke
som eksterne e-mails.

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
fase; genstart afventer ny kontrol.

**Ret** gennemgår et forkert link, efter at den oprindelige postering er
tilbageført og en korrekt erstatning er bogført særskilt. Serveren kræver
den faktiske tilbageførselsrelation og kontrollerer kilde, beløb og historik.
Bekræftelse med begrundelse gemmer en ny afstemningshændelse og en varig
kvittering; den foretager ikke selv tilbageførsel eller ny bogføring.
CLI/MCP tilbyder samme plan og bekræftelse med den oprindelige nøgle.
Den samlede første serverfase (Group18/19) har verificeret web og CLI/MCP,
herunder kvitteringer og afvisninger. Genstartskontrollen for rettelser er
endnu ikke nået i den samlede kørsel.


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
bekræftelseslinks; der er ikke sendt ekstern e-mail i kontrollen.

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

Alle disse første forløb har bestået mod den rigtige server (Group19). Den
samlede genstartskontrol afventer den igangværende kørsel; kørselskontrollens
rækkefølge er rettet, så rapportens gemte totaler omfatter alle eksempelrejser.

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
