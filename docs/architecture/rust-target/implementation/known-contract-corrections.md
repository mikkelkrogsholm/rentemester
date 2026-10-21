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
