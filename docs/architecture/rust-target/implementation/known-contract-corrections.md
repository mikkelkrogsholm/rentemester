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
