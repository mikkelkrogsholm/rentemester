# Kendte kontraktrettelser i Rust-omskrivningen

Dette register skelner mellem kildeadfærd, som skal bevares, og påviste fejl,
som skal have en eksplicit korrigeret kontrakt. Det tilføjer ingen nye
produktfunktioner til omskrivningen. Grundlaget er R01/#673, R19/#694 og
[epicens grænser](../epic/EPIC.md), som kræver kildeparitet og afvisningsbeviser
uden automatisk at importere åbne forbedringer som nyt scope.

## Renter på en valutafaktura

Den fastlåste kildebaseline er
`0bbe82bcf0cafd73e2326152740cb06b90222405`, jf.
[baselineinventaret](baseline/inventory.json).
`src/core/schema.sql:2014–2033` kræver DKK på originalfakturaen ved
rentebogføring, konkret linje 2024. Positiv rentebogføring på en valutafaktura
var derfor ikke en understøttet kildeoperation.

Kildens renteberegning brugte derimod den oprindelige hovedstols enheder uden
en eksplicit valutakonvertering, mens beløbet blev benævnt DKK. Det er ikke en
valutakontrakt, som Rust må videreføre som en korrekt beregning. Hverken den
oprindelige R19-accepttekst eller GDD’en kræver en ny positiv
valuta-rentebogføring; R19 kræver portering af de understøttede forløb og
korrekt afvisning af ugyldige kombinationer. Den senere implementeringsstatus’
formulering om et ubetinget nyt foreign-interest-krav var en for bred
planfortolkning og er korrigeret.

Den native kontrakt er derfor:

- DKK-beregning, registrering, bogføring og kausal korrektion understøttes.
- Alle fem rentehandlinger på en valutafaktura afvises eksplicit med
  `unsupported_foreign_interest`; ingen fejlagtig DKK-beregning, krav,
  postering eller auditmutation oprettes.
- Valutafakturaens eksisterende hovedstol, betalinger og separate
  DKK-kompensation fortsætter som selvstændige understøttede forløb.
- En eventuel fremtidig positiv valuta-rentefunktion kræver en selvstændig
  valuta-/kurs-/korrektionskontrakt og er ikke en skjult forudsætning for
  at færdiggøre omskrivningen.

Den konkrete lagringstest er
`invoice_interest::refusals::foreign_interest_actions_refuse_explicitly_without_accounting_or_claim_effects`.
Den opretter og bogfører en faktisk syntetisk EUR-faktura gennem native
writers og kontrollerer alle fem afvisninger, uændrede journal-/krav-/audit-tal,
fortsat læsbar EUR-hovedstol og ren fakturaudit. Motorens tilsvarende
valutaafvisning dækkes også af de eksisterende motortests.

## Ratekilde ved beregning af tidligere rentekrav

Kildens almindelige beregning indlæste ikke tidligere kravs
`reference_rate_source`, mens korrektionsberegningen gjorde. Et tabelkrav over
et halvårsskift kunne derfor få forskellig lovlig rente ved almindelig
beregning og korrektion. Native beregning og korrektion bruger én fælles
kildebevidst vinduesfunktion. Dette ændrer ingen rentetabel eller manuelt
valgt rate. Den målrettede motorregression er
`table_halfyears_and_old_claims_share_the_same_windows`.

## Fakturaens momssats og bogføringsklassifikation

Kildens `src/core/vat-lines.ts` og `issued-invoices.ts` understøtter positive
momssatser på fulde fakturaer og beregner de konkrete linjebeløb. Linjens
`vatRate` har forrang; `totals.vatRate` er en fallback. Rust bevarer denne
beregning ved udstedelse, PDF, læsning og tilbagevendende generering, inklusive
blandede satser. En fuld faktura med grundlag100 og sats0,2 har derfor moms20
og total120; den må ikke ændres til125 ved næste periodes generering.

Kildens `invoice-booking.ts` mærker derimod alle taxable linjer som
`DK_SALE_25`. Det er en regnskabsfejl, når satsen ikke er25%. Den native
momsrapport forventer25% af netop dette grundlag. Rust skelner derfor mellem
validerede fakturabeløb og en understøttet bogføringsklassifikation. En ukendt
klassifikation giver `unsupported_invoice_vat_classification` før ny
bogføring eller kreditnota. Dokumentet kan fortsat udstedes, læses og indgå i
backup. Revisionskontrol og kreditnotaernes historiske gren må heller ikke
opfatte en forkert25%-mærkning som en gyldig tilladelse til nye posteringer.
De eksisterende25%-, fritagelses- og reverse-charge-koder bevarer deres
regnskabsmæssige betydning. Reduced simplified-fakturaens indeholdte25%-kontrakt
bevares særskilt.

## Valgfri fritekst ved omvendt betalingspligt

Kildens validator og den eksisterende positive differentialfixture tillader
en eksplicit reverse-charge-basis uden `reverseChargeNote`. Rusts krav om
både basis og fritekst var derfor en porteringsfejl. Struktureret basis,
korrekt retning, de nødvendige linjebeslutninger, nulmoms og afstemte totaler
kræves fortsat. En angivet fritekst valideres og bevares; ved udeladelse viser
nye PDF'er standardteksten om omvendt betalingspligt fra den eksplicitte
beslutning. Allerede gemte originale filer genbruges uændret.

## Virksomhedssti og actor i serverklienter

Kildens lokale CLI kunne vælge et virksomhedsrod-katalog og tilskrive en lokal
actor. I den aftalte serverarkitektur åbner klienten ikke en ledger. Remote
vælger derfor en slug, og identitet følger serverens aktuelt autentificerede
cookie eller serviceprincipal. Actor-miljøvariabler kan ikke udgive sig for en
anden bruger eller give serverrettigheder.

Group158 bevarer et afgrænset lokalt stivalg gennem en eksplicit verificeret
`--local-profile`: et eksisterende kanonisk, ikke-symlink virksomhedsrod-katalog
direkte under profilens workspace omsættes til slug. En eventuel workspace-
miljøassertion skal stemme. Det giver samme servervej for lokal og ekstern drift
med en tydelig afvisning af stier, som serveren ikke har optaget. Det er en
bevidst arkitekturtilpasning; ikke en påstand om identisk lokal actor- eller
standardlauncheradfærd. Øvrig kildeparitet og cutover skal fortsat accepteres.

## Periodepacket og lokal CLI-force

Det eksisterende native close-packet har version4 og format
`rentemester.native-period-close.v1`. Det binder også lukningen til den
faktiske journal-/audithovedrevision og de native fysiske kontroller. Den
oprindelige version3 genindføres ikke som en parallel, svagere beregning.
De originale periodeklienter bevarer hele det versionerede native packet og
hash. Gemte version3-reviews kan læses uændret; et historisk review giver ikke
automatisk tilladelse til en ny lukning.

Kildens lokale CLI har desuden et force-mismatch: parseren behandler flaget
som boolesk, mens handleren spørger efter teksten yes/true. Lokal actor giver
aldrig trusted force-authorisation. Rust afviser derfor eksplicitte CLI-force-
forespørgsler med brugsfejl, så de ikke ignoreres og bliver til en almindelig
lukning. Den oprindelige autentificerede MCP-vej kan have en levende force-
rettighed. Dens positive waiver-/obligationsforløb er stadig et åbent
porteringskrav; CLI-rettelsen fjerner det ikke fra epicens scope.


## Manuel journal og serverejet attribution

Group160 accepterer originale numeriske journal-input gennem den fælles Rust-
writer. Caller-felterne createdBy/createdByProgram afvises eksplicit. Aktuel
serverprincipal ejer attribution; hverken fri tekst eller et import-programnavn
giver identitetsrettigheder eller fritagelse fra bilagskravet. CLI-kaldet er en
oprindelig, unkeyed handling; der genereres ingen skjult nøgle. Oprindelige MCP-
værktøjers eksplicitte idempotencyKey er fortsat et separat porteringskrav.
Native forretningsafvisninger bevarer deres faktiske errors/appliedRules og får
en manglende transportkode fra den konkrete fence; kontrollerne ændres ikke.


## Oprindelig MCP-journal og native receipts (Group161)

De tre oprindelige MCP-navne, numeriske payloads, bekræftelse, envelopes og
valgfri eksplicit nøgle er nu accepteret gennem samme Rust-motor. Group160s
sikre selectorprioritet bevares også her: ID, derefter præcist nummer, derefter
entydig tekst med filtre. Kildens LIMIT1/fallback må ikke reversere en anden
postering ved tvetydig tekst eller et ukendt eksplicit nummer.

Den nye receipt binder det validerede rå business-input med formatet
`rentemester-original-journal-mcp/1`, før selectors opløses. Det er ikke en
påstand om samme hashbytes som gamle TypeScript-receipts. Den eksisterende
import-/restore-reconciliationkontrol bevares, og historiske nøgler må ikke
genudstedes. Kodeløse kildeafvisninger kan fortolkes ved HTTP200; HTTP422
kræver fortsat en gyldig serverkode. Regnskabs- og adgangskontroller svækkes
ikke for at opnå transportparitet.


## Autoriseret oprindelig MCP-force og principaltype (Group162)

Group159s force-afvisning på den oprindelige CLI bevares. Den særskilte
autentificerede MCP-vej har nu kildeadfærd med levende force-rettighed,
præcist review og uforanderlige åbne waiver-obligationer. En frisk periode
kan force-lukkes direkte som reported, mens en eksisterende lukket periodes
report-overgang fortsat afviser force. Reported forbliver terminal.

Den native v4-packetkontrakt og de eksisterende native readiness-rettigheder
bevares. En menneskelig subject kan lovligt starte med service-account:, så
principaltype udledes aldrig af denne tekst. Den sendes typet fra serverens
verificerede indgang og bevarer hele subject-ID'et i review og audit.


## Undtagelser efter genåbning (Group163)

TypeScript-kildens loadArchivedBoundary filtrerer lagret accounting_periods.status,
selv om genåbning er append-only og kan efterlade den lagrede status closed.
De oprindelige Rust CLI/MCP-interface bruger derfor samme effektive
periodestatus som native web og resolver. Genåbnede opgaver bliver synlige
igen. Cutover er fortsat strengt før datoen; effektiv closed/reported dækker
inklusive grænser, og udaterede opgaver arkiveres ikke ved gæt. Dette er en
afgrænset kildefejlrettelse, ikke en separat arkivmodel. Markering af den
operationelle undtagelse ændrer aldrig de uforanderlige waiver-obligationer.


## Kontakter og aktuelle eksterne observationer (Group164)

Original `customer validate-vat` har en vildledende TTL-beskrivelse, men
kildens faktiske operation kalder provideren hver gang. Rust-porten bevarer
den operation og gemmer også en autoritativ valid:false-observation; native
foreign-invoice-VIES-regler er uændrede. Serviceadgang følger de aktuelle
Owner/Bookkeeper-roller. Human VAT bruger den eksisterende Fresh-sessiongate
både før og efter providerarbejdet, frem for at lade en blot Current-session
slippe gennem den friske mutationsgrænse.

Originale kontaktlister skal samtidig overholde den eksisterende
GDPR-redaktion. Rust bruger den fælles redaktion, returnerer kildens felter
uden det ekstra gdprErased-metadatafelt og sorterer de synlige værdier efter
SQLite-kompatibel ASCII lower(name)/ID. Det geneksponerer ikke skjulte navne
og udleder ikke sorteringsorden fra de uredigerede persondata. Leverandørens
oprindelige frie kontofelter svækker ikke fælles motors kontoplan- og VAT-kontrol.


## Oprindelige bilagskontroller og historisk evidens (Group165)

Rust følger den erklærede eksplicitte bekræftelse for parse/parse-pending,
selv om den gamle CLI-implementering havde glemt denne gate. Klientens lokale
fil kontrolleres efter bekræftelsen og sendes som bytes; en MCP-filreference
giver ikke serveren adgang til en vilkårlig sti. Service- og menneskeadgang
udledes af de aktuelle credentials gennem fælles mutation/read-kontrol.

Virksomhedskontekst må kun kvalificere præcis det valgte dokument. Omvendt
opslag efter ens metadata kunne både låne en anden originals kontekst og
afviste lovlige selvstændige bilag; det er erstattet med dokument-ID,
originalhash, payloadhash og snapshotbinding. Den historiske virksomhedsprofil
valideres efter sin egen fulde form og hash, så en senere legitim ændring af
navn, adresse eller momsperiode ikke gør en gammel ledger ugyldig.

Ny supplering af metadata efter enhver partshistorik ville ændre den payload,
som en eksisterende eller supersederet partshændelse har bundet. Det afvises
før skrivning; samme allerede eksisterende enrichment-hash forbliver en
autoriseret no-op. Historik- og backupkontrollen bevares. En regression
reproducerer det tidligere brud og beviser både afvisning uden ændringer og
den lovlige no-op samt fortsat gyldig backup.

### Oprindelige partskoblinger og restoreproveniens (Group166)

Original planfejl returnerer et fejlet MCP-envelope; den tidligere ydre succes
ved fejlet plan bevares ikke. Source-historik skjuler idempotency_key; native
historik beholder sit eksisterende felt. CLI actor/principal er valgfrie
lighedsassertioner mod aktuel serveridentitet og giver aldrig myndighed.
Historisk gyldig agent/system-attribution og forskellige actor/principal
bevares strukturelt uden at blive genindført som aktuel adgang.

Et beskrivende navneudsagn ændrer ikke den kildefastsatte kanoniske
partssnapshot. Faktisk omplanlægning afprøves derfor med en eksplicit tilladt
issuer-rolle og en ny evidensbundet plan; en gammel supersederet kvittering
kan ikke genoplive koblingen. Metadataenrichment efter partshistorik afvises
fortsat af Group165-kontrollen.

Gendannelse tilføjer bevidst native_restore_receipt_boundary. Prøven kræver
alle oprindelige auditposter og domænerækker eksakt bevaret og præcis én
ny post med korrekt næste ID, type, entity, high-water-mark, manifesthash,
actor og tidsstempelformat. Seks isolerede manipulationsprøver afviser
ændret historik, manglende/ekstra post, falsk actor, forkert manifest og
ændrede domænedata. Ingen generel auditfiltrering eller svækket kontrol.

### Originalt fakturaudtræk og aktuel serviceidentitet (Group167)

Et kildekompatibelt udtræk returnerer flad `{ok, extraction}`; MCP lægger kun
`extraction` i sit normale envelope. Et positivt ID uden dokument eller
udtræk giver `extraction:null` efter aktuel adgangskontrol, mens native manglende-ID-politik
bevares. Konflikt/providerfejl med gemt evidens er et vellykket udtræksresultat,
som kræver afklaring. Den præcise deaktiverede providerfejl er en kendt
afvisning; et mistet eller uklart skriveresultat genforsøges aldrig automatisk.

En services eksplicit tildelte subject, nøgle, scope og aktuelle rettigheder
bestemmer dens adgang. Den historiske menneskelige opretter er auditspor,
ikke en kæde af nedarvet aktuel myndighed. Tests tilbagekalder derfor det
faktiske subject eller dets nøgle/scope; ingen ny creator-cascade indføres.

Den faktiske prøve bevarede to fejlede kørsler: serviceoprettelse svarer korrekt
HTTP 201, og et konfliktudtræk blev først bestilt via MCP og derefter fejlagtigt
bestilt igen af hjælperen. Den første statusforventning er rettet præcist; den
anden fase læser det allerede gemte konfliktresultat. Begge rettelser blev
kontrolleret isoleret på de fangede resultater før en ny samlet prøve. De seks
providerkald og begge konfliktforsøg fra den fejlede kørsel bevares; de skjules
ikke som cache eller rulles tilbage. Produktets udtræks- og adgangspolitik er
uændret af disse hjælperrettelser. Gendannelse bruger fortsat Group166's
eksakte auditpræfiks og ene manifest/high-water-bundne restorepost.

### Oprindelig udgift, valgfri kvittering og præcis momsprøve (Group168)

De oprindelige to CLI- og tre MCP-operationer er additive adaptere til samme
Rust-motor. CLI accepterer seks momsbehandlinger; kildens MCP accepterer fem.
En providerfejl kan være en gemt, flad preflight med `ok:false`; rå providerdata
returneres ikke. Et ukendt skriveresultat genforsøges ikke automatisk.
MCP's valgfrie idempotensnøgle kontrolleres først efter aktuel adgang og
fysisk kilde. Konflikt returnerer kildens definite afvisning og bevarer
den eksisterende diagnostiske audit uden ny journal eller bankafstemning.

Den første faktiske prøve stoppede korrekt ved et konfliktbilag uden
obligatorisk beskrivelse og modtager. Fem originaler og fire bankposter
bevares i den fejlede prøve; der var ingen journalposter eller VIES-kald.
Hjælperens metadata blev sammenlignet isoleret med den gemte, accepterede
EU-faktura før en ny prøve. Indlæsning af en konfigureret PDF afventer allerede
udtræk; hjælperen læser derfor det gemte resultat i stedet for at bestille
et nyt udtræk. Ingen dokument-, udtræks- eller adgangskontrol er svækket.
Den endelige prøve kvalificerer præcis DK-standard og EU-reverse-charge;
den hævder ikke en faktisk prøve af alle seks behandlinger eller desktop.

### Kildens periodiseringsdatoer, nullable bilag og nulperioder (Group169)

Kildens faktiske postering af forudbetalt omkostning og forudbetalt indtægt
rammer kun balancekonti ved registrering og kan derfor undlade bilag. Prosaen
om at forudbetalt indtægt altid rammer resultatet er upræcis. Senere udgifts-
eller indtægtspostering kræver stadig fælles journalvaliderings bilag. Nullable
hoveder kan læses gennem både oprindelige og native rapporter. Et angivet
bilags fysiske original og hash kontrolleres altid.

Kilden kræver ikke registrering før første plandato eller en eksplicit
periodiseringsdato efter plandatoen. De oprindelige adaptere følger denne
adfærd; native wrappers bevarer deres tidligere relative datopolitik. Fælles
aktuel bogføringshorisont, lukket/indberettet periode, konti, dubletter og
balancering bevares. Ingen separat bogføringsmotor indføres.

Planen 0,01 kr. over to perioder er 0 og 0,01 kr. Den kan registreres og
rapporteres, men nulpostering afvises fortsat af fælles journalvalidering.
Afslutning bestemmes af antal bogførte perioder og må ikke udledes af en
resterende saldo på nul. Asset-motorens positive periodelogik er ikke ændret.
Tekniske crate-header-, float-literal- og ækvivalente prædikatfejl blev rettet
afgrænset med bevarede checkkvitteringer før den første faktiske prøve.
Ingen finansielle skrivninger blev gentaget for at teste transporten.

### Dokumenteret kapitalisering og straksafskrivning (Group170)

Oprindelig aktivregistrering genbruger den eksisterende Rust-motors kontrol
af dokumenteret, ufordelt kapitalisering. Nettoberegningen medregner fortsat
korrektionsposters omvendte linjer; et filter på kun `posted` ville fejlagtigt
fjerne deres krediteringer og tillade for stor fordeling. Registrering
opretter kun aktivets metadata. Native afskrivning før anskaffelsesdato
forbliver afvist. Kildens nulperioder kan planlægges og læses; fælles
journalvalidering afviser stadig en faktisk nulpostering.

Straksafskrivning har ingen opfundet relativ datogrænse før kildens
durable undtagelser. Fælles journalvalidering håndhæver fortsat aktuel
bogføringshorisont, konti, låste perioder og bilag ved faktisk postering.
Beløb som afrundes til nul afvises før bilagsopslag med samme positive
pengeinvariant som lagringen. Kun en positivt konstateret manglende
bilagsrække må give dokumentationsundtagelsen; en ændret eller ulæselig
fysisk original afvises uden en sådan undtagelse. Den fysiske kontrol
bevares også før en rådgiverundtagelse om beløbsgrænsen.

MCP's ydre `confirm` er valgfri i schemaet, men manglende eller falsk
bekræftelse afvises før serverkald. Den indre `confirmImmediateWriteOff`
er en påkrævet boolean; falsk er en legitim forretningsafvisning.
Fejlede checks og reviewrettelser blev bevaret, og den første faktiske
prøve bestod uden gentagelse af finansielle skrivninger. Ingen prøve
påstår juridisk egnethed eller udleder momsfradrag af de syntetiske køb.

### Lokal kørselseksport og eksisterende eksportret (Group171)

Kildens MCP-beskrivelse af eksport til en servermappe er korrigeret: filer
tilhører den lokale CLI-/stdio-maskine, og outputDir sendes aldrig som sti
til Rust-serveren. Serveraudit beskriver forberedte data frem for at påstå
lokal filsucces. Eksisterende faste filer kan erstattes som i kilden;
per-fil-atomicitet er ikke en garanti for samlet crash-atomicitet.

Manglende eller ugyldige numeriske CLI-værdier udelades på wire og når
serverens ordnede kildevalidering; eksplicit null er ikke et tal. Negative
og nulværdier giver stadig forretningsafvisning. Kildens positive kilometer,
som afrundes til nul, afvises fortsat af den eksisterende Rust-kontrol.
Eksportens JS-tal 0 og serverens monetære wiretal 0.0 sammenlignes i præcise
hundrededele, så tomme og heltallige summer ikke afvises som usikre resultater.

CompanyExport tillades allerede for læsere. Den faktiske prøves oprindelige
forventning om afvisning var forkert; data og audit fra den prøve bevares.
Den korrigerede prøve tilbagekalder medlemskabet før eksportafvisning og
tilbagekalder derefter nøglen. Ingen adgangspolitik er strammet eller svækket.

Flere historiske MCP-discoverytests har stadig faste gamle totaler på bl.a.
225, 257, 319, 322 og 325 tools. Det er en konkret resterende harnesskorrektion
ved næste registrytilføjelse: genbrug den eksisterende fælles
EXPECTED_NATIVE_TOOLS, og bevar kontrollen af alle navne, schemaer og kald.
Den fulde stdio-suite påstås ikke bestået af denne afgrænsede delaccept.

### Budgetprognoser, native overload og faktisk testopsætning (Group172)

Det oprindelige forecast gættede bankkonti ud fra navne og kunne vise nul uden
bevist mapping. Rust bevarer den eksisterende bekræftede mapping og afviser
manglende åbningsgrundlag. Den månedlige kilde kunne også lægge udenlandske
beløb til DKK. Uden dateret kurs afvises denne månedsprognose; den ugentlige
prognoses eksplicitte udeladelser bevares. Ingen ny kursleverandør er indført.

Ugebudgetter uden betalingsdato er information og påvirker ingen cashlinje.
Mellemregningsscenarier læses fra aktuelt valideret kontrolhistorik i samme
admitterede transaktion; godkendelse er ikke bevis for betaling eller
finanspostering. Native årssammenligning bevarer sit eksisterende envelope,
mens den oprindelige periodesammenligning bevarer sin source-kontrakt.

Gamle stdio-totaler er erstattet med den eksisterende fælles forventning uden
at fjerne navne-, schema- eller kaldskontroller. En gammel banktest pegede på
det tidligere endpoint/envelope; den er korrigeret mod den faktiske binding
og kontrollerer nu også hele fejlen, tekst og isError. Alle 168 tests består.

Prøvernes opsætningsfejl er isoleret frem for at svække produktkontroller:
parrede transaktionstests kræver normale filbaserede SQLite-forbindelser;
native kreditorregistrering er POST /payables; en tabel bruger
invoice_document_id som nøgle; den eksisterende private scanner skal både
aktiveres og kende præcis de tre syntetiske filnavne. Kravet om tre scanninger
er bevaret. Hver fejlet kvittering og private profil er bevaret, inklusive
allerede udført bogføring eller signeret gendannelse.

### Momsprojektioner og fastholdt kildehistorik (Group173)

Source-salgsmoms inkluderer bad-debt-justeringen; native bruttofelt alene
ville vise et forkert tilsvar. De fulde source-grundlag udledes derfor af
samme native akkumulering uden at ændre den eksisterende serialisering.
Optælling af tilbageførte originaler afhænger af modposteringer i det
valgte interval, så en senere tilbageførsel ikke omskriver en tidligere rapport.

Source MCP-fejl udelader data. EU-success kræver sin præcise regel, og filing
kræver det tilstedeværende nullable referencefelt. Et eksisterende OSS-navn
får én source-binding. Historisk import er fortsat en eksplicit manglende
kildevalidering, som afviser sikkert; ingen ny feltprojektion ophæver den.

Før den faktiske prøve blev to opsætningsforventninger rettet mod koden:
ældre fixtureår vælges eksplicit, og en bogført bankpost efterlader en løst
opgave med kildehenvisninger og resolutionshistorik. Den må ikke forventes
slettet. Samlet accept bestod første gang uden at svække nogen kontrol.

### Eksplicit EU-køb og konfigureret repræsentationskonto (Group174)

Der findes to oprindelige moms-skriveadaptere: EU-servicekøb og repræsentation.
Non-EU-køb har ingen separat original CLI/MCP-registrering. EU-adapteren må
derfor ikke blot auto-vælge den generelle expense-adapters udenlandske
behandling; den har en eksplicit EU-leverandørkontrol og læser cached VIES.

Source3070-defaultet er erstattet med en unik aktiv, eksplicit konfigureret
REPRESENTATION_SPECIAL-konto eller krav om expenseAccountNo. Det bevarer
inputformatet uden at indbygge en bestemt kontoplan. Betaling bruger fortsat
bekræftet bankrolle, og ingen bankpost opfindes ved køb uden bankreference.

En successfuld SDK-kvittering skal indeholde operationens kanoniske momsregel;
et generisk journalresultat må ikke anerkendes som afsluttet momsbehandling.
Falske resultater bevarer tomme regler og udelader MCP-data. Ugyldige
dubletleverandør-/VIES-testidentiteter og obligatoriske compile/lint-fejl blev
rettet uden svagere kilde-/regnskabskontroller. En fejlagtig hovedagent-
indvending om credential-rækkefølge er eksplicit trukket tilbage; den
oprindelige funktionelle adgangskontrol var korrekt og er bevaret.

### Kvalificeret historisk moms og lærte kontroller (Group175)

Den oprindelige globale programmarkør er utilstrækkelig som kildebevis. Kun
eksisterende validerede Dinero-importjournaler kvalificeres. Momskode på linjen
vinder; gennemgåede kontodefaults bruges kun for kvalificeret historik. Lært
kontobetydning gælder også native journaler med eksplicit grundlag og giver
hverken importtillid eller et automatisk grundlag.

Det dokumenterede Dinero64040/64060-par fortolkes kun med attestering, korrekt
kontotype og præcist lige modstående beløb. En bekræftet modstridende rolle
afvises; kreditnormal liability alene er ingen outputrolle. Generisk omvendt
moms følger kildens særskilte én-øre input/output-afvigelser og deres maksimum;
det særskilte Dinero-par beholder sit lighedskrav. Repræsentationsgrundlag
gendannes kun fra den verificerede form. Beløbskorrektioner og almindelig/
omvendt moms afstemmes hver for sig uden at ændre den oprindelige journal.

En indledende indvending om reversed-status blev trukket tilbage: native
importbevis kræver posted-original, og tilbageførsel sker med en spejlpost.
Status-/hashkrav er bevaret. Ugyldige syntetiske transaktions-, dokument-,
VAT-grundlags-, JSON-tal- og FK-antagelser blev diagnosticeret fra konkrete
fejl og rettet i fixtures. Ingen regnskabskontrol blev lempet.

### Kildebundne leverandørforpligtelser og entydige MCP-navne (Group176)

Den tidligere TypeScript-plan brugte ren inputhash og en anden kalender.
Det oprindelige CLI/MCP-interface anvender nu den eksisterende native plan:
aktuel aktiv leverandør, originale kildebytes, proveniens, gennemgået input og
forrige hash bindes sammen. Horisonten er næste dato plus90dage inklusive;
månedsforekomster beholder deres oprindelige datoanker. Et angivet
leverandørsnapshot er en lighedsassertion, aldrig autoritet. Ikke-aktiv
startstatus afvises. Ingen native kilde- eller adgangskontrol svækkes.

Kildens ændringskommando uden forventet hash slår den aktuelle aktive hash
op i samme adgangskontrollerede transaktion. Pause kan derfor ikke afsluttes
direkte via dette interface; ny aktivering kræver et nyt gennemgået forslag.
Det native interface beholder sin udtrykkelige forventede hash.
Kvitteringsgenfinding med kendt oprindelig nøgle går gennem native idempotens
efter aktuel adgang og fysisk kildekontrol; en ny plan må ikke afskære en
allerede gemt kvittering efter afslutning. Ingen automatisk mutation gentages.

Den oprindelige `supplier_commitment_matches`-form og den tidligere native
form får hver sit navn; nativeformen findes under
`native_supplier_commitment_matches`. Discovery indeholder348unikke navne.
Aktør og principal kommer fra aktuelle bruger-/servicecredentials og gemmes
med korrekt type; gamle native brugerkald bevarer deres kontrakt.

To misvisende testforventninger blev rettet fra konkrete resultater før
godkendelse: historisk registry-tal341 efter syv nye bindings og antagelsen
om direkte afslutning af en pause. Sidstnævnte blev rettet før første faktiske
prøve ved at bevare afvisningen og kræve gennemgået genaktivering.

### Dimensionsplaner, præcise beløb og historiske identiteter (Group177)

MCP-planer bevarer kildens indlejrede succesform; en afvist kerneplan er nu
også en ydre fejl, ikke en succes med indlejret `ok:false`. Liste- og
skrivekvitteringer beholder deres egne kildeformer. Budgetbeløb afstemmes
i præcise øre uden tolerancer eller afrunding. Kanoniske numeriske hashes
stemmer med uafhængige kildefixtures; hele gamle budgetfordelinger afløses
atomisk, og procent/underscore i idempotensnøgler behandles bogstaveligt.

Både den gamle og den native klassifikation sletter `sourceRef` for reviewed.
En identisk plan kan derfor ikke oprettes igen efter supersession med blot
en ny reference. Prøven bevarer i stedet en virkelig ekstern fordelingsplan
i samme original og gennemgår dens import eksplicit med originalens SHA.
Det er import af klassifikation, ikke bogføring; den unikke immutable hash
og femhændelsesforløbet er bevaret.

SDK'en normaliserer valuta som native motor og bevarer gyldige historiske
rækker, hvor aktør og principal er forskellige. Nye mutationer afleder begge
fra aktuelle credentials. Den samlede request inklusive identitetsassertioner
begrænses til65KiB. Ingen klient kan vælge autoritet gennem disse felter.

### Regelforslag, historik og hypotetisk forklaring (Group178)

Den native kanoniske regelpayload er autoritativ og kan genberegnes fra den
bevarede version. Creator og createdAt indgår ikke: TypeScript-kildens Pick-type
fjernede ikke disse runtimefelter fra hashinputtet. Klientstyret createdAt
afvises; serveruret bestemmer audit. Lifecycle effectiveAt og læsningens at
understøtter validerede UTC-datoer/timestamps og en samlet gyldig historik.
Supersede afslutter en godkendt version uden implicit efterfølger. Identisk
payload genfinder samme version uden nye hændelser.

Alle fire CLI-skrivninger kræver nu eksplicit --confirm yes. Nye aktører kommer
fra aktuelle credentials; actor/principal kan kun være lighedsassertioner.
En afvist kerneoperation bliver en præcis422fejl gennem authority-fencen.
Forklaring/test beholder kildens flade form uden ok/errors; human_decision er
en vellykket læsning. MCP har kun propose/approve/explain og bevarer deres
kildeenvelopes. Historisk null/rejected tilstand er legitim; matched kræver
godkendt tilstand og tomme mismatchgrunde. Bodygrænsen er65536bytes i alle lag.

Generelle regler deler eksisterende historik og backupkontroller. Den native
purchaseparser forbliver streng: amount, reverse-charge, andre attributes og
textTemplate må ikke accepteres og derefter ignoreres ved faktisk bogføring.
Forklaring af en hypotese er ingen postingautoritet; understøttede faktiske
purchases bruger stadig deres uændrede bevis- og anvendelseskontrol.

### Kontoplan, kildeprojektion og historiske rolleaktører (Group179)

Kildens fire kontooperationer bruger små fælles typed helpers i company_setup.
Nye identiteter kommer fra aktuelle credentials; native human wrappers beholder
deres hidtidige kald. Den oprindelige eksplicitte rollebekræftelse opretter en
ny version selv ved samme konto, mens den native wrapper beholder idempotens.
Kildeprojektionen viser otte krævede roller; de to valgfrie FX-roller bevares
i det native forløb. Forslag er diagnostik og giver ingen autoritet.

Kontoflagget indsættes atomisk og bruger én eksisterende account_created-audit
i stedet for TypeScript-navnet accounts_add. Flagget beskytter de eksisterende
afgrænsede bad-debt/claim-forløb; der er ikke indført en generel journalpolitik.
En gyldig source-created false-konto afvises af den eksisterende bad-debt-kontrol,
mens samme fixture lykkes med en almindelig udgiftskonto. Den generelle manual
postingforskel er fortsat åbent produktscope og hævdes ikke løst her.

SDK skelner mellem historisk proveniens og nutidige credentialassertioner:
native_seed/system samt bevarede typed agent/system-aktører kan læses; de
giver ingen adgang i dag. Inputnormalisering bevarer trimmede kontofelter og
kildens case-insensitive true/false. MCP Add har appliedRules på ydre niveau;
role-confirm bevarer coreok i data.ok, og CLI-afvisninger bevarer rollediagnostik.
Begge CLI-skrivninger kræver eksplicit --confirm yes.

### Oprindelige kladder og præcis godkendelseskvittering (Group180)

Source-adapteren genbruger accounting_drafts::change/list og journal_post.
En lille intern receiptsink bevarer det faktiske første postresultat uden at
ændre den native DTO. Replay læser samme journal-id/nummer/hash og returnerer
tomme appliedRules som kilden; der foretages ingen ny postering for at
rekonstruere en kvittering. Den native strengere replayregel bevares: kun
den oprindelige aktuelle reviewer med præcis submitted-hash og confirmation.

Fem CLI-skrivninger kræver nu --confirm yes, og businessfalse giver exit1.
--input læser en begrænset, rekursivt unik lokal JSON-fil; stien sendes aldrig
til serveren. Actor/principal er kun nutidige credentialassertioner. Bevaret
agent/system-attribution kan læses uden at give adgang eller omskrive historiske
hashes. Native payload-/eventhashkæde og kildeevidens er fortsat autoritative.
Sourceprojektionen udelader native nullreason/journal-id, og missing show er null.

Sourceforløbet er syv CLI-kommandoer og ingen oprindelige MCP-tools. Create/revise/
submit kræver DraftWrite, reject/approve Review og reads Read, altid før parsing.
Kendte strukturelle afvisninger er400; native businessafvisninger er422, og
usikkert commitudfald forbliver503uden automatisk retry. Savepointen bevarer
hele-operationens rollback selv ved sen fejl efter journalinsert.

### Normale native programnavne og fælles outputdefault (Group181)

Kandidatens `rentemester` er en uafhængig bytekopi af Rust CLI-binæren. Der er ingen
Bun-/Node-wrapper eller fallback. Den eksisterende Docker-buildidentity og
desktopopdatering ændres ikke; kandidatens eget inventar er ikke en releasegodkendelse.
Fælles original-argv-normalisering vælger human på stdout-TTY og JSON ved pipe.
`--json` vinder; ellers vinder sidste `--format`. Offline discovery, native `remote`,
eksperimentelle kommandoer og bogstavelige værdier efter `--` bevarer deres kontrakt.

R16's sidste navnekollisionskriterium er nu prøvet gennem de normale native
programnavne: samme leverandørnavn, forskellige juridiske ids, separate vendorIds,
ingen overskrivning og præcis historik efter genstart. Kontoplan-/party-/reviewbeviser
fra30/31/60/164/166/179 bevares på deres egne revisioner/snapshots. Historiske #524
(chart-templates), #624 (generel bilagsmetadata-supersession) og #641 (knowledge
service-/path-scope, fortsat under R30) er særskilt åbne og erklæres ikke implementeret
ved afslutning af R16. Ingen regnskabs-, adgangs- eller restorekontrol svækkes.
