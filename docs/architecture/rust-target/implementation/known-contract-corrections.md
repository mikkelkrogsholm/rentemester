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
