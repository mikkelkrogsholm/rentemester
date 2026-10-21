# Dinero-import på Rust-serveren

Åbn virksomhedens administrationsside, vælg **Dinero-kildepakke**, og angiv
en særskilt importnøgle. **Gennemgå Dinero-import** viser serverens fortolkning,
åbningsposter, historiske bilag og kontrolsaldi. Gennemgå oplysningerne og
bekræft importen. En kontrolforskel på én øre blokerer bogføringen.

Den aktuelle indlæsning tager en JSON-kildepakke:

```json
{"format":"rentemester-dinero-source/1","currency":"DKK","files":[{"path":"2025/Kontoplan.csv","contentBase64":"..."}]}
```

Medtag de originale CSV-filer med deres relative stier: firmaoplysninger,
kontoplan, posteringer og saldobalance samt den eventuelle fil
`Rentemester-modtagerposter-v1.json`. Filernes præcise bytes bevares i pakken.
Webvisningen pakker endnu ikke en mappe eller ZIP-fil automatisk.

Importen opretter konti og forslag til kontoroller, bogfører det seneste års
åbning og bilag og gemmer kildeoversigt, accepteret importforsøg og eventuelle
importerede tilgodehavender i én transaktion. Kontoroller kræver fortsat
bekræftelse. Tidligere års understøttede CSV-historik gemmes i arkivet og
bogføres ikke igen i det aktive år. En eksisterende åbning eller modstridende
kontoplan kræver afklaring, før importen kan gennemføres.

Importerede tilgodehavender vises på fakturasiden med en valgfri
opgørelsesdato. Visningen medtager også fuldt betalte kildefakturaer og viser
kildefilens betalings- og kreditnotahændelser på deres datoer. Importen
udsteder eller sender ikke fakturaerne på ny. Dokumenthash i ledsagefilen er
kildens erklæring; bevarelse af pakken autentificerer ikke eksterne bilag.

## HTTP, CLI og MCP

Alle stier nedenfor ligger under `/api/companies/{slug}/`:

| HTTP | CLI `remote` | MCP |
| --- | --- | --- |
| `POST imports/dinero/plan` | `dinero-import-plan` | `dinero_import_plan` |
| `POST imports/dinero/apply` | `dinero-import-apply` | `dinero_import_apply` |
| `GET imports/dinero` | `dinero-imports` | `dinero_imports` |
| `GET imports/dinero/{attemptId}/original` | `dinero-import-original --attempt-id ID` | `dinero_import_original` |
| `GET invoices/imported-receivables?asOf=YYYY-MM-DD` | `imported-receivables --as-of YYYY-MM-DD` | `invoice_imported_receivables` |

Plan tager `{fileName,contentBase64}`. Apply tager de samme felter plus
`planHash`, `idempotencyKey` og `confirm:true`. CLI læser JSON fra stdin;
MCP modtager JSON-tekst i `body`. Apply kræver også klientens ydre
bekræftelse. Originalruten returnerer rå `application/json`; CLI og MCP
returnerer indholdstype og præcise bytes i deres sædvanlige svar.

Kildepakken må højst være 25 MiB med 4.096 entydige, sikre relative filstier.
En enkelt udpakket fil må højst være 16 MiB, og den samlede filstørrelse
højst 18 MiB. Serveren udleder aktør, import-ID, posteringer og kontroller.
Bogføring kræver aktuel menneskelig MFA, virksomhedsadministrator og en
opfyldt backup-lås. Læse- og dokumentrettigheder kontrolleres særskilt.

Ved mistet svar blokerer webvisningen en ny skrivning. Brug **Kontrollér
status**, og afstem importforsøg, journal og kildeoriginal manuelt. En bevidst
gentagelse med præcis samme nøgle og indhold kan læse den oprindelige
kvittering; den kræver fortsat aktuel adgang og intakte kildeoriginaler.
En ændret nøgle er ikke en sikker gentagelse af et uafklaret forsøg.

## Verifikationsgrænse

Lagerets integrationstests dækker atomisk bogføring og tilbagerulning,
kontrolforskel, forældet plan, kildeændringer, genåbning, betalte kildeposter,
tidligere års arkiv, momsåbning og fælles debitorkontrol for importerede og
nye fakturaer. Den samlede prøve gennem eksisterende web, HTTP, kompileret
CLI/MCP, gentagelse med samme nøgle, fuld audit og genstart består. Den
uafhængigt verificerede kørsel er registreret som `native_dinero_import124` i
projektets fremdriftsfil. Prøven omfatter signeret backupkontrol, men ikke
gendannelse uden kildeprofil eller installeret desktopaccept.

Denne leverance afslutter ikke alle oprindeligt aftalte importvarianter,
bilagskoblinger, rapportintegrationer eller afklaring af migrerede åbne poster.
Den installerede desktopapps manuelle prøve udføres til sidst.
