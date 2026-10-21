# Regnskabskontrol i Rust

Regnskabskontrollen sammenholder journal, revisionsspor, bankkoblinger,
fakturagrundlag og registrerede originalfiler. Den læser data og ændrer ikke
posteringer eller virksomhedens revisionsspor. Den reparerer ikke automatisk.

I web-UI'et findes den under **Integritet → Regnskabskontrol → Kontrollér
regnskabet**. Samme web-UI bruges af Rust-desktopappen.

## HTTP, CLI og MCP

- `GET /api/companies/{slug}/audit-verify` har ingen query-parametre eller body.
- `rentemester-rust remote audit-verify --company SLUG --server URL --cookie-file FILE`
- MCP: `audit_verify` med `{ "company": "SLUG" }`.

CLI/MCP understøtter også `--service-key-file FILE`. Brug én credentialtype.
En menneskelig bruger kræver en aktuel MFA-session og `CompanyRead`.
Serviceadgang kræver aktuel nøgle, virksomhedsscope, læseret og kvote.
Tilbagekaldte rettigheder afvises ved den aktuelle læsning. Kun eksplicit lokal
udvikling kan bruge `--allow-loopback-http`.

Et HTTP-resultat har `ok: true` og objektet `audit`. CLI og MCP placerer dette
resultat i `data`. Kommandoens succes betyder, at observationen kunne leveres;
regnskabets resultat findes i `audit.ok`, `audit.available`, `audit.errors` og
`audit.components`.

`audit.format` er `rentemester-native-audit-verification/1`. Rapporten indeholder
virksomhedens `slug`, antal `entries` og fem komponenter:

| Komponent | Kontrol |
| --- | --- |
| `ledger` | Journalhashes, linjer, balance, tilbageførsler, sekvenser, revisionsspor, native kvitteringer og historisk proveniens |
| `bank` | Direkte og append-only koblinger, korrektionskæder, beløb, konti og kildereferencer |
| `invoices` | Faktura-, kredit-, betalings-, refusions-, krav-, tabs- og importerede tilgodehavenderelationer |
| `originals` | Registrerede bilag, faktura/credit-originaler, bankoriginaler og historiske arkiver i det ejede filnamespace |
| `physical_schema` | Aktuelt understøttet databaseschema og SQLite integrity check |

Hver komponent har `available`, `ok`, navngivne `checked`-tællere og `errors`.
Alle fem skal være tilgængelige og uden fund, før `audit.ok` bliver sand.
Manglende kontrolgrundlag giver en ufuldført kontrol; fund giver et negativt
regnskabsresultat. Webvisningen rydder et tidligere resultat ved en ny forespørgsel
eller adgangsfejl. Fund vises som tekst.

## Afgrænsning

Kontrollen bruger de dokumenterede persistente kilder og deres historiske
kontrakter. En imported receivable schedule gemmer ikke sit oprindelige samlede
JSON-dokument i de eksisterende tabeller. Dens `schedule_hash` kan derfor kun
sammenholdes på tværs af de gemte headers/events og importforsøg; den oprindelige
samlede schedule kan ikke gendannes og rehashes alene fra disse tabeller. Dette er
ikke en attestering af den manglende kildefil.

Native periodelukning har særskilte krav og fortsat eksplicitte begrænsninger for
endnu ikke fuldt understøttede skriveforløb. En ren regnskabskontrol ophæver ikke
disse krav. SQLite- og namespacekontrol er ikke en attestering af værtsmaskinens
hardware, deployment eller ekstern backup. Den installerede desktopapps visuelle
accept udføres til sidst.

## Verificeret forløb

Den syntetiske acceptprøve bruger almindelig mailverifikation og MFA. Efter én
bogføring og tilbageførsel sammenlignes deterministiske rapporter for to
virksomheder gennem HTTP, CLI og MCP, og den første virksomhed sammenlignes
også med en scoped servicekonto. En almindelig læser får adgang, mister den
ved tilbagekaldelse og får sin oprindelige rolle tilbage.

Den faktiske webvisning viser både rene og fejlede kontroller. Ændringer af
et ubogført originalbilag og en uafstemt bankoriginal opdages; genetablering af
de præcise bytes giver den oprindelige rapport. Posteringer, bankdata, rapporter
og virksomhedens revisionsspor ændres ikke af kontrollen. Efter genstart og nyt
MFA-login er rapporterne identiske.

Målrettede Rust-tests dækker befolkede journal-, bank-, kredit-, betalings-,
krav-, tabs- og importkilder samt manipulation. Det samlede end-to-end
regnskabs-, valuta- og lifecyclekorpus og den nye samlede kontrol efter
source-free restore kræver fortsat særskilt accept. Den installerede desktoptest
ligger til sidst.
