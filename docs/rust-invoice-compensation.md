# Kompensationskrav på Rust-serveren

Åbn en bogført faktura og vælg **Rykkergebyrer og krav → Kompensationskrav**.
Forløbet har adskilte trin:

1. **Beregn kompensation** for en valgt opgørelsesdato. Det ændrer ingen data og
   viser, om den eksisterende kompensationsregel kan anvendes, samt et eventuelt
   allerede registreret eller bogført krav.
2. **Registrér kompensation** efter gennemgang og bekræftelse. Registreringen
   opretter ét krav på fakturaen; den opretter ingen postering. Registreringsdatoen
   kan ikke ligge efter den aktuelle danske dato.
3. **Bogfør kompensation** efter valg af dato, tilgodehavendekonto og
   indtægtskonto. Der bogføres i DKK uden moms. Datoen må ikke ligge før kravdatoen,
   og normale periode-, adgangs-, original- og backupkrav gælder.
4. Afstem fakturaens hovedstol via det almindelige fakturaforløb. Afstem derefter
   en faktisk DKK-bankindbetaling på det bogførte restkrav via kravpanelet.

Der sendes ingen e-mail eller opkrævning automatisk. Et dubleret krav eller en
allerede udført bogføring afvises. Ved ukendt udfald skal den aktuelle status
læses og afklares; der gives ingen automatisk genkaldelsesgaranti.

## Beløb og valuta

Porteringen bruger den eksisterende versionerede kompensationspolitik med
standard/maksimum 310 DKK og startdato 2013-03-01. Der kræves en positiv,
inddrivelig hovedstol på en forfalden, bogført faktura samt den eksisterende
kommercielle køberidentifikation: CVR/VAT-format, EAN-format eller eksplicit
`publicRecipient`. Formatkontrollen er ikke et eksternt registeropslag.

Kompensationen er et DKK-krav, også når fakturaens hovedstol er i en anden valuta.
Hovedstolen vises i fakturaens valuta; krav og kravbetalinger vises i DKK. Der
lægges aldrig forskellige valutaenheder sammen. I kravstatus er
`claimOpenBalance` derfor `null` for valutafakturaer; særskilte felter angiver
`principalCurrency`, `claimCurrency`, `registeredClaimBalanceDkk` og
`ledgerBackedClaimBalance`. For DKK-fakturaer bevares den kombinerede DKK-saldo.

Tilgodehavende- og indtægtskonti vælges eksplicit. Produktet indsætter ikke et
bestemt kontoplannummer som standard. Fakturaens oprindelige valutakurs og
DKK-værdi forbliver grundlaget for hovedstolens normale afvikling.

## HTTP, CLI og MCP

Alle tre operationer bruger `POST`, JSON-body på højst 16 KiB og ingen query:

| Operation | HTTP-sti efter `/api/companies/{slug}/` | CLI `remote` | MCP |
| --- | --- | --- | --- |
| Beregn | `invoices/compensation/calculate` | `calculate-invoice-compensation` | `invoice_compensation_calc` |
| Registrér | `invoices/compensation/register` | `register-invoice-compensation` | `invoice_claim_compensation` |
| Bogfør | `invoices/compensation/post` | `post-invoice-compensation` | `invoice_post_compensation` |

Beregning tager `invoiceDocumentId`, `asOfDate` og valgfrit
`compensationAmountDkk`. Registrering tager de samme felter, valgfri `note` og
`confirm:true`. Bogføring tager `invoiceDocumentId`, valgfri `transactionDate`,
`receivableAccountNo`, `compensationIncomeAccountNo` og `confirm:true`.
Ukendte felter, klientvalgt aktør og idempotensheaders accepteres ikke.

CLI læser den præcise JSON-body fra stdin og kræver `--company`, `--server` og
`--cookie-file`. Skrivninger kræver desuden det bare flag `--confirm`. MCP bruger
`company` og `body` som JSON-tekst, plus `confirm:true` ved skrivninger. SDK bruger
`InvoiceCompensationOperation` og `ApiClient::invoice_compensation`.

Svaret er `{ok:true,compensation:…}`; CLI/MCP placerer det i `data`. Beregningen
har `eligible` og `reason`; et succesfuldt beregningskald er ikke en tilladelse til
at registrere et krav. Eksisterende krav og postering fremgår af nullable
`claimId`, `claimDate`, `registeredAmountDkk`, `postedJournalEntryId` og
`postedJournalEntryNo`. Aktuel autoritet og regnskabsgrundlag kontrolleres igen
ved hver skrivning.

Der kræves en aktuel menneskelig MFA-session: `CompanyRead` til beregning,
`CompanyDraftWrite` til registrering og `CompanyLedgerPost` til bogføring og
betaling. Disse ruter accepterer ikke en servicenøgle som menneskelig session.

## Databaseschema og periodekontrol

Den valgfrie native kompensationsudvidelse installeres atomisk ved en bekræftet
bogføring. Den historiske schema- og migrationshistorik ændres ikke. Udvidelsen
har en uforanderlig identitet og ændrer kun reglen, der tidligere begrænsede
originalfakturaens valuta til DKK. Journalens DKK-beløb, konti, dato, dokument,
status, linjer og tilbageførselstjek bevares. Et delvist eller ukendt schema
accepteres ikke som denne udvidelse.

Periodekontrollen bruger de konkrete kompensations- og betalingsbeviser.
Fortsatte begrænsninger for renter, tab og andre endnu ikke porterede forløb
ophæves ikke. Den samlede [regnskabskontrol](rust-audit-verify.md) kontrollerer
også de nye krav og betalinger.

## Verifikation

Den samlede prøve med almindeligt login og MFA består på syntetiske data:
DKK-faktura via web og EUR-faktura med CLI-registrering og MCP-bogføring,
hovedstolsbetaling i fakturavaluta og separat DKK-kravbetaling. HTTP, CLI og MCP
returnerer samme beregning og kravstatus. Prøven kontrollerer også aktuelle
læser-/revokerede rettigheder, dubletter, bekræftelser, datoer, konti og den
aktuelle backup-lås uden utilsigtede regnskabsændringer.

Slutdata indeholder to fakturaer med fire originalrækker (JSON og PDF), fem
banktransaktioner og ni posteringer. Hovedstol og krav er afviklet, alle fem
komponenter i regnskabskontrollen er grønne, originalbytes er uændrede, og den
præcise status består efter genstart og nyt MFA-login. Den nye signerede backup
er desuden verificeret uafhængigt med serverprogrammet og den betroede nøgle.

Rapport: `/tmp/rentemester-rust-reports/native-invoice-compensation120c.json`,
SHA-256 `73bc0a38e85f329d11e4236d666d8b5c35a8d4bc5af657a4e43e2c0809681c7f`.
Målrettede motor-, storage-, adgangs-, SDK-, MCP-, register- og webtests samt
webbygning og streng Clippy består også. Storage-forløbet kontrollerer både
fakturaudit og periodens konkrete kompensationsbeviser.

Denne prøve kvalificerer ikke kildefri gendannelse af den nye schemaudvidelse,
servicekontoskrivninger, faktisk afsendelse eller installeret desktop-GUI.
Privat køber er dækket af syntetiske policytests, men ikke et særskilt faktisk
webforløb. Den samlede resterende regnskabs-/valuta-/restorematrix og hele
Rust-omskrivningens øvrige leverancer er fortsat åbne. Desktopaccept udføres sidst.
