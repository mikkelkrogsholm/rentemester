# Konstateret tab på Rust-serveren

Tabsafskrivning findes i fakturaens fælles webvisning under **Rykkergebyrer og
krav → Konstateret tab**. Vælg dato og en konto til tab på debitorer, angiv
valgfrit et delbeløb i fakturaens valuta, og bekræft, at tabet er konstateret og
dokumenteret. Gennemgå handlingen før bogføring. Serveren afgør ikke selv,
om dokumentationen juridisk etablerer tabet.

Udeladt beløb betyder hele den aktuelle åbne hovedstol. Serveren udleder de
oprindelige debitor- og momskonti og bogfører tabsudgift, momsregulering og
lettelse af debitoren atomisk. Den accepterer kun det understøttede
standardmomsgrundlag og et positivt beløb inden for den dokumenterede saldo.
Rykker-, rente- og kompensationskrav beholder deres selvstændige saldi og kan
fortsat afstemmes med faktiske indbetalinger.

For valutafakturaer vises både fakturavaluta og faktiske DKK-beløb. Debitoren
lettes efter den resterende bogførte DKK-værdi; momsreguleringen bruger det
oprindelige momsgrundlag. Efter en delvis afskrivning bruger senere betalinger
den resterende bogførte værdi til kursgevinst eller kurstab. Serveren bruger
eksakte beløb og kontrollerer tidligere posteringer og bevarede grundlag.

## HTTP, CLI og MCP

`POST /api/companies/{slug}/invoices/bad-debt/write-off` tager:

```json
{"invoiceDocumentId":7,"writeOffDate":"2026-10-21","grossAmount":312.50,"expenseAccountNo":"6230","note":"Dokumenteret tab","confirm":true}
```

ID og konto er eksempler og skal vælges fra den konkrete virksomhed.
`grossAmount` og `note` er valgfrie. Svaret er `{ok:true,writeOff:...}` med
originale beløb, faktiske DKK-beløb, journalreference og resterende hovedstol
og krav. Den aktuelle faktura- og kravstatus indeholder `badDebtWriteOffs`,
`totalBadDebtWrittenOff` og `totalBadDebtWrittenOffDkk`.

CLI: `rentemester-rust remote write-off-invoice-bad-debt` med server,
virksomhed, privat cookie-fil, JSON fra stdin og `--confirm`.
MCP: `invoice_write_off_bad_debt` med `company`, JSON-tekst i `body` og ydre
`confirm:true`. Begge kræver også bekræftelsen i selve JSON-handlingen.

Aktuel menneskelig MFA og bogføringsret kræves. Serveren kontrollerer
originalfaktura, kreditnotaer og eksisterende betalingsoriginaler samt
backup-lås, perioder, konti og betroet dansk dato. Servicekontoskrivning er
ikke tilgængelig for denne handling. Klientvalgt aktør, momskonto,
debitorkonto og idempotensnøgle accepteres ikke.

Ved mistet svar blokerer webvisningen gentagelse. Brug **Kontrollér status**
eller genlæs faktura, krav og journal, og afstem det faktiske udfald manuelt.
En ny skrivning er en ny bogføring, ikke en sikker gentagelse. Der afsendes
ingen opkrævning automatisk. Kilden har ingen særskilt funktion til genvinding
af allerede tabsafskrevet hovedstol; en sådan funktion indføres ikke her.

## Verifikation

Målrettede tests dækker delvise og fulde tab, kumulativ momsafrunding,
valutagrundlag, senere betaling og kursregulering, bevarede krav,
periodekontrol og genåbnet database. Fejlagtige konti, datoer, beløb,
rettigheder og ændret historik afvises. Den samlede faktiske prøve består gennem web, CLI og MCP: to særskilte tab
på 312,50 DKK, resterende hovedstol 625 DKK og kompensation 310 DKK betales;
EUR-fakturaens resterende 74,95 EUR afskrives med 557,03 DKK i debitorlettelse
og 111,41 DKK i momsregulering. Ti journalposter, fire bankposter og fire
originale JSON/PDF-bilag består den fulde regnskabskontrol. Aktuelle rettigheder,
backup-lås, mistet accepteret svar uden gentagelse, præcise originaler og
genlæsninger, nyt MFA-login efter genstart og uafhængig kontrol af signeret
backup er verificeret.

Rapport: `/tmp/rentemester-rust-reports/native-invoice-bad-debt122b.json`,
SHA-256 `ed95e3e46fbc842a405c57bf4e9101c09068a124518be3ce820f9b38c01a65fb`.
Den første fejlede prøve bevares som `native-invoice-bad-debt122.json`: den
fandt manglende udgiftskonti i kravstatus, før nogen tabsafskrivning var bogført.
En regressionstest mod det faktiske lagringssvar og den rettede samlede prøve
dækker rettelsen.

Denne prøve kvalificerer ikke gendannelse uden kildeinstansen eller uafhængig
juridisk etablering af tabet.
Den installerede desktopapps afsluttende test er fortsat særskilt.
