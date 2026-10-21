# Renter og rettelser på Rust-serveren

DKK-forløbet er verificeret gennem Rust-serveren og det fælles web-UI med
almindeligt login og MFA, genstart og uafhængig kontrol af en signeret backup.
CLI og MCP er med i præcis genlæsning og afvisninger. Valutafakturaer har en
korrigeret afvisningskontrakt for renter. Den samlede omskrivnings øvrige
leverancer er fortsat åbne som beskrevet nedenfor.

## Brugerforløb

1. Beregn rente på en bogført faktura for en valgt opgørelsesdato. Udeladt
   referencerente bruger produktets eksisterende versionerede rentetabel;
   en eksplicit referencerente er et manuelt valg. Beregning ændrer ingen data.
2. Registrér et positivt rentekrav efter gennemgang. Registreringen opretter
   ingen journalpost og er adskilt fra bogføring.
3. Bogfør et valgt ubogført krav med eksplicit tilgodehavende- og rentekonto.
4. Hvis en efterfølgende registreret betaling har en tidligere effektiv dato,
   beregn et korrektionsforslag. Serveren udleder grundlag og oprindelige konti.
   Kun sikkert ubetalt og utvetydigt rentegrundlag kan krediteres automatisk.
5. Bogfør en gennemgået korrektion med begrundelse. Afstem hovedstol og en
   faktisk indbetaling på det resterende rentekrav via faktura-/kravforløbet.

Der afsendes ingen opkrævning automatisk. Ved ukendt udfald læses den aktuelle
krav- og journalstatus; en skrivning genkaldes ikke blindt.

## Beregning og beviser

Motoren bruger eksakte ørebeløb og simpel rente uden renters rente. Den deler
renteperioder ved effektive hovedstolsændringer og rentetabellens skift. Hele
numeratoren summeres før afrunding; der afrundes ikke separat pr. delperiode.
Betalinger og kreditnotaer reducerer hovedstolen fra deres effektive dato,
refunderinger øger den, og dokumenterede nedskrivninger reducerer den.

Alle registrerede krav er periodeankre, også ubogførte krav. Beregning og
korrektion bruger samme kildebevidste rentevinduer. Historiske opslag bevarer
den eksisterende kildekontrakt for senere krav og afkortede vinduer.

En korrektionsberegning giver et matematisk loft. Lagerlaget skal derudover
bevise journaler, originale konti, tidligere allokeringer og sikkert ubetalt
rente. En gammel gyldig korrektion bliver ikke ugyldig, fordi en senere
registreret refundering øger den lovlige rente og sænker nutidens overkrav.
Den historiske plan kontrolleres i sin oprindelige kausale sammenhæng.

Rentehandlingerne understøtter DKK, ligesom kildens bogføringsregel.
På valutafakturaer afvises alle fem rentehandlinger eksplicit med
`unsupported_foreign_interest`, så et beløb i fakturavaluta ikke fejlagtigt
vises som DKK. Det er en [dokumenteret kontraktrettelse](architecture/rust-target/implementation/known-contract-corrections.md).
Eksisterende valutafakturaers hovedstol og DKK-kompensation bevarer deres
særskilte forløb. Ny positiv rente på valutafakturaer er en særskilt fremtidig
produktfunktion; den opfindes ikke som forudsætning for omskrivningen.

## HTTP, CLI og MCP

Alle handlinger bruger JSON med `POST` efter `/api/companies/{slug}/`:

| HTTP | CLI `remote` | MCP |
| --- | --- | --- |
| `invoices/interest/calculate` | `calculate-invoice-interest` | `invoice_interest_calc` |
| `invoices/interest/register` | `register-invoice-interest` | `invoice_claim_interest` |
| `invoices/interest/post` | `post-invoice-interest` | `invoice_post_interest` |
| `invoices/interest/correction` | `calculate-invoice-interest-correction` | `invoice_interest_correction_calc` |
| `invoices/interest/post-correction` | `post-invoice-interest-correction` | `invoice_post_interest_correction` |

Beregning tager `invoiceDocumentId`, `asOfDate` og valgfri
`referenceRatePercent`. Registrering tilføjer valgfri `note` og `confirm:true`.
Bogføring tager `invoiceDocumentId`, valgfri `claimId` og `transactionDate`,
`receivableAccountNo`, `interestIncomeAccountNo` og `confirm:true`.
Korrektionsforslag tager `invoiceDocumentId`; bogføring af rettelsen tager
samme ID, valgfri `transactionDate`, en `reason` og `confirm:true`.
Klienten vælger ikke korrektionsbeløb, konti eller kravallokeringer.

De første tre svar indeholder `interest`; de to korrektionssvar indeholder
`interestCorrection`. `hasProposal` er matematisk. `canPost`, `reason`,
`requiresRefund` og `proposedLines` beskriver den beviste bogførbarhed.
Kravstatus kan genlæses fra beregningens `claims` og fakturaens almindelige
kravstatus. CLI bruger JSON fra stdin; MCP bruger JSON-tekst i `body`.
Skrivninger kræver også klientens ydre `--confirm`/`confirm:true`.

Aktuel menneskelig MFA kræves: læseret til beregninger, kladderet til
registrering og bogføringsret til posteringer og rettelser. Normal kontrol af
originaler, backup-lås, periode og betroet dansk datohorisont gælder.
Ukendte felter, klientvalgt aktør og idempotensheaders accepteres ikke.

## Verifikation

Den faktiske prøve opretter et rentekrav på 2,47 DKK uden automatisk bogføring,
bogfører det efter gennemgang og registrerer derefter en delbetaling på
500 DKK med tidligere effektiv dato. Den lovlige rente bliver 1,78 DKK;
korrektionen på 0,69 DKK bogføres alene på de oprindelige konti. Derefter
afvikles den resterende hovedstol og den faktiske renteindbetaling på 1,78 DKK.

Prøven slutter med syv journalposter, fire banktransaktioner og to
originalbilag (JSON og PDF). Alle fem komponenter i regnskabskontrollen er
grønne, originalbytes er uændrede, og status er identisk efter genstart og nyt
MFA-login. HTTP, CLI og MCP giver samme beregning, korrektionsforslag, kravstatus
og originaler. Aktuelle læser-/revokerede rettigheder, backup-lås, bekræftelser
og dubletter afvises uden utilsigtede regnskabsændringer. Skrivesucces i denne
faktiske prøve går gennem web-UI’et; CLI/MCP dækker genlæsning og skriveafvisninger.

Rapport: `/tmp/rentemester-rust-reports/native-invoice-interest121.json`,
SHA-256 `9caef3b5771f01dbfe6cf8ee67f47e712aabcb578b2c2c0a2b8149a3c7716051`.
Ni motortests, fire storageforløb og én autoritetstest, den konkrete
periodekontrol og en close/reopen-regression, to klienttests, MCP- og
registertests samt otte webtests består. Webbygning og streng Clippy for de
otte berørte crates består. Periodens tidligere rente-blanket er erstattet
af konkrete krav-/korrektions-/betalingsbeviser.

Tests dækker betalt/tvetydigt rentegrundlag og en faktisk kredit/refundering,
som bevarer en tidligere gyldig korrektion. Disse negative forløb er ikke
særskilt kvalificeret via det faktiske webforløb. Kildeuafhængig gendannelse,
servicekontoskrivninger og installeret desktop-GUI er heller ikke kvalificeret
af denne prøve. Den eksisterende bad-debt-grænse afviser stadig et ikke
understøttet principalgrundlag eksplicit; den ignorerer ingen nedskrivning.
Den korrigerede valutaafvisning er dokumenteret særskilt; desktopaccept udføres sidst.
