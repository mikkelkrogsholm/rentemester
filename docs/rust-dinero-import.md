# Dinero-import på Rust-serveren

Åbn virksomhedens administrationsside, vælg en **Dinero-eksportmappe** eller
en rå ZIP-fil under **Dinero-kildepakke**, og angiv en særskilt importnøgle. **Gennemgå Dinero-import** viser serverens fortolkning,
åbningsposter, historiske bilag og kontrolsaldi. Gennemgå oplysningerne og
bekræft importen. En kontrolforskel på én øre blokerer bogføringen.

JSON-kildepakker understøttes fortsat:

```json
{"format":"rentemester-dinero-source/1","currency":"DKK","files":[{"path":"2025/Kontoplan.csv","contentBase64":"..."}]}
```

Medtag de originale CSV-filer med deres relative stier: firmaoplysninger,
kontoplan, posteringer og saldobalance samt den eventuelle fil
`Rentemester-modtagerposter-v1.json`. Filernes præcise bytes bevares i pakken.
Mappevælgeren danner denne pakke af filernes præcise bytes og relative stier.
ZIP-filer sendes uændret til Rust-serveren og bevares med deres egen rå hash,
størrelse og filinventar. En gendannet JSON-pakke er ikke ZIP-originalen.
ZIP-input understøtter ukomprimerede og Deflate-komprimerede filer.
Krypterede arkiver, andre komprimeringsmetoder, ugyldige stier, symlinks,
kolliderende filnavne og integritetsfejl afvises.

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

## Medfølgende bilag

Importen medtager det seneste kildeårs PDF-, PNG- og JPEG-filer under
`<år>/Bilag/` og `<år>/Faktura/`. Filnavnets præcise bilagsnummer skal findes
i kildeposteringerne. Bilagene kobles til de importerede posteringer og vises
på den eksisterende bilagsside. Posteringernes indhold og hash ændres ikke.
Fakturaoriginaler genudstedes ikke.

Filer under `Ikke-bogførte-bilag/` bevares med en opgave til gennemgang i
undtagelseslisten. Identiske bytes genbruger et eksisterende dokument, mens
hver kildesti fortsat har sit eget proveniensspor. Ukendte bilagsnumre og
ikke-understøttede bogførte filtyper afvises før importen.

Bilagsudvidelsen består den samlede prøve gennem web, HTTP og kompileret
CLI/MCP, med præcise originaler, bilagskoblinger, undtagelse, gentagelse med
samme nøgle, fysisk ændringskontrol, fuld audit, signeret backupkontrol og
genstart. Evidensen er registreret som `native_dinero_supporting125` i
fremdriftsfilen. Gendannelse uden kildeprofil og installeret desktopaccept
er endnu ikke kvalificeret for denne udvidelse.

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
bekræftelse. Originalruten returnerer rå `application/json` eller `application/zip`; CLI og MCP
returnerer indholdstype og præcise bytes i deres sædvanlige svar.

JSON-pakken eller den rå ZIP-fil må højst være 25 MiB med 4.096 entydige, sikre relative filstier.
En enkelt udpakket fil må højst være 16 MiB, og den samlede filstørrelse
højst 18 MiB. Serveren udleder aktør, import-ID, posteringer og kontroller.
Bogføring kræver aktuel menneskelig MFA, virksomhedsadministrator og en
opfyldt backup-lås. Læse- og dokumentrettigheder kontrolleres særskilt.

Ved mistet svar blokerer webvisningen en ny skrivning. Brug **Kontrollér
status**, og afstem importforsøg, journal og kildeoriginal manuelt. En bevidst
gentagelse med præcis samme nøgle og indhold kan læse den oprindelige
kvittering; den kræver fortsat aktuel adgang og intakte kildeoriginaler.
En ændret nøgle er ikke en sikker gentagelse af et uafklaret forsøg.

## Lokal mappe eller ZIP via CLI

`remote dinero-import-plan --file PATH` læser en lokal eksportmappe eller ZIP
og viser serverens plan. En mappe pakkes deterministisk; ZIP-originalen ændres
ikke. Den lokale filsti sendes ikke til serveren.

`remote dinero-import-apply --file PATH --plan-hash HASH --idempotency-key KEY --confirm`
kræver den allerede gennemgåede plan og en eksplicit nøgle. Angiv desuden de
sædvanlige server-, cookie- og virksomhedsflag. Klienten planlægger eller
forsøger ikke automatisk igen. Hvis kilden ændres mellem gennemgang og
bekræftelse, afvises planen. Den eksisterende stdin-JSON-kontrakt fungerer
fortsat, når `--file` ikke er angivet.

Mappe- og ZIP-udvidelsens integrerede tests og samlede rå-ZIP-prøve består.
Web, HTTP og kompileret CLI/MCP bevarer den præcise ZIP-original gennem
bogføring, gentagelse med samme nøgle, fuld audit, signeret backupkontrol og
genstart. Rapportens hash og den uafhængige kontrol er registreret som
`native_dinero_containers126` i fremdriftsfilen. Den faktiske mappevælger er
kvalificeret til gennemgang; en samlet bogføring fra mappevælgeren,
gendannelse uden kildeprofil og installeret desktopaccept er ikke kvalificeret.

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
rapportintegrationer eller afklaring af migrerede åbne poster.
Den installerede desktopapps manuelle prøve udføres til sidst.


## Det eksisterende syntetiske CSV-format

Rust-serveren læser også det oprindelige `synthetic-csv`-format gennem
åbningsbalancens eksisterende plan- og apply-ruter. Beløb i CSV-filen er hele
øre, fx `150000` for 1.500,00 kr. Formatet har ingen CSV-citering:

```csv
# cutOverDate: 2025-07-01
section,accountNo,name,debit,credit
account,A,Eksempel på aktiv,,
account,E,Eksempel på egenkapital,,
opening,A,,150000,
opening,E,,,150000
```

Klassificér kontiene eksplicit i den almindelige kontoplan først. CSV-formatet
angiver kun nummer og navn; serveren gætter ikke kontotype eller momskode.
Navne kan tilpasses før første postering. Konti med posteringer beskyttes.

`remote opening-balance-plan --file PATH` sender den præcise UTF-8-fil til
serverens gennemgang. Bogføring kræver
`remote opening-balance-apply --file PATH --plan-hash HASH --idempotency-key KEY --confirm`
samt de sædvanlige server-, cookie- og virksomhedsflag. Der udføres ingen
implicit planlægning eller automatisk gentagelse. Den samme serverkontrakt er
tilgængelig via `opening_balance_plan` og `opening_balance_apply` i MCP:
input er `{sourceSystem:"synthetic-csv",content:"..."}`, og apply pakker dette
som `{input,planHash,idempotencyKey,confirm:true}`.

Kilden må højst være 48 KiB og JSON-anmodningen 64 KiB. Præcise UTF-8-bytes,
inklusive BOM og linjeskift, bevares i åbningsposteringens uforanderlige
auditdokumentation og den signerede databasebackup. Kontotilpasning,
åbningspostering og kildebevis gemmes atomisk med den eksisterende Rust-motor.
Den almindelige webvisning viser også denne bogførte åbningsbalance.
Målrettede tests består; den samlede importerede ejerprøve er endnu ikke
kvalificeret.
