# Roadmap

Rentemester bygges som en reliability-loop: hvert lag skal være troværdigt før det næste lægges ovenpå. Roadmap'en er ikke en fast plan — den er en prioriteret liste over hvad jeg synes der skal ske, i den rækkefølge jeg synes giver mening. Den ændrer sig.

Hvis du vil bidrage til noget specifikt: åbn et issue eller skriv på det relevante eksisterende.

---

## Hvad der virker i dag

Hele den deterministiske kerne er på plads og dækket af tests. Det her er det fundament alt andet bygges ovenpå:

- **Bogføring**: dobbelt bogholderi med append-only journal og hash-kæde. Reverseringer i stedet for rettelser. Hvert entry har audit-spor med eksplicit aktør.
- **Fakturering**: udstedelse, bogføring, PDF-generering, kreditnota, rykker, rente, kompensation, tab på debitorer, multi-valuta.
- **Bank**: CSV-import med kolonne-mapping, suggest-matches, reconciliation, settlement (inkl. kombineret principal + rykker/rente).
- **Bilag**: ingest med SHA-256, leverandør-stamdata, expense-bogføring, VIES-validering af EU-numre og fremmedvaluta-køb med DKK-bankafregning.
- **Moms**: rapport, EU reverse charge, repræsentation, bad-debt relief, TastSelv-rubrik-mapping.
- **System**: signed backups (HMAC + opt-in ed25519 til 3.-parts revisor-verifikation), restore med audit-chain-verifikation, periodelås, 5-års retention, myndighedseksport, første deterministiske SAF-T-slice, deterministisk lokal bogholder-/revisor-handoff-pakke og en byte-deterministisk compliance-rapport (`compliance report`) med sha256-fingerprint som revisor kan re-køre for at bekræfte uændret tilstand.
- **Agent-grænseflade**: MCP-server med 81 tools, agent-agnostisk (Claude, Mistral, Ollama lokalt, eller intet).
- **Dashboard**: statisk HTML, deterministisk, bygger på DESIGN.md-tokens.
- **CI**: `bun test` + `bun run smoke` håndhævet på hver PR.
- **Lovgrundlag**: alle regler i `rules/dk/*.yaml` med SHA-256-citation mod retsinformation.dk's faktiske XML-tekster.

Hvis det her ikke virker, er ingen feature ovenpå troværdig. Det virker.

---

## Hvad jeg arbejder på næst

Listet i prioriteret rækkefølge — ikke alt sker samtidigt, og rækkefølgen kan ændre sig hvis nogen kommer ind med kvalificeret input.

### Bilagsmail (e-mail-bilagsindgang)

README lover det. Idéen: en dedikeret e-mail-adresse pr. virksomhed; vedhæftede bilag (PDF, JPG, EML) ingestes automatisk; metadata hentes via parsing eller agent; usikre cases ryger i exception-køen.

Skal udvikles uden lock-in til en specifik mail-provider — IMAP eller cataloggeret mailserver med plain disk-ingest.

### Gentagne fakturaskabeloner før abonnementsautomatik

Udstedte fakturaer, betalinger og rykkerflow er på plads. Det næste naturlige lag er ikke "subscription SaaS", men en lille deterministisk kerne for gentagne fakturaer:

- en template med kunde, linjer, moms, interval og næste dato
- eksplicit generering for en given `asOfDate`
- blokering af dubletter for samme template/periode
- audit-spor fra genereret faktura tilbage til template

Det skal løse den reelle hverdag for freelancere og små ApS'er uden at opfinde baggrundsautomation eller skjult tilstand.

### Bogholder-/revisor-review af regelfortolkning

Den største kvalitetsforbedring lige nu er at få en der faktisk har lavet bogføring til at læse `rules/dk/*.yaml` igennem. Især momsbehandling i grænsetilfælde (delvis fradrag, blandet økonomi, særlige brancher).

Kontakt mig direkte hvis du er kvalificeret og motiveret.

### Bogholder-/revisoradgang ud over eksportpakken

Første slice er nu en deterministisk lokal eksportpakke (`system export-accountant`) med eksplicit trust boundary: fil-handoff, ikke live adgang. Næste lag er evt. hosted reviewer/accountant access med tydelige roller og audit-attribution — men det er stadig roadmap, ikke noget README må oversælge.

### Live web-UI

Statisk HTML-dashboard er det første lag. En interaktiv version — hvor brugeren kan klikke en faktura åben, redigere en udgift, godkende en exception — er næste skridt.

Stack-valget er ikke truffet. Skal være lille, deterministisk, og overholde DESIGN.md.

### GDPR-kommandoer

Data-export og kontrolleret sletning er lovkrav når Rentemester ender hos rigtige brugere. Kerne-retention-mekanismen er der; CLI-kommandoer mangler:

- `rentemester gdpr export --subject "kunde-CVR" --out <dir>`
- `rentemester gdpr forget --subject "kunde-CVR" --after-retention-expiry`

Skal håndhæve at sletning kun er mulig efter retention-pligten er udløbet.

### PSD2 / åbne bank-API'er

CSV-import virker, men friktionen forsvinder først når bank-transaktioner kan hentes direkte. Lunar har en åben API; Nordea/Danske Bank kræver PSD2-flow.

Skal designes så Rentemester aldrig opbevarer bank-credentials i klartekst — kun OAuth-tokens med refresh.

### E-faktura til det offentlige (PEPPOL)

Fakturaer til offentlige kunder kan markeres med public-recipient metadata og eksporteres som et gyldigt **Peppol BIS Billing 3.0**-dokument. Transport-sømmen (`transmitPublicEInvoicePeppol`) er på plads: en injiceret transmitter afleverer dokumentet, og udfaldet registreres idempotent og auditbart.

Det sidste trin er den ægte transport via et selv-hostet access point (Oxalis). Det kræver et MitID systemcertifikat og en endpoint-registrering i NemHandelsRegistret, som kun virksomhedsejeren kan skaffe — ikke en kommerciel tredjepart. Hele opsætningen er dokumenteret i [docs/peppol-nemhandel.md](docs/peppol-nemhandel.md).

Relevant når der er brugere der har offentlige kunder.

### Bredere SAF-T og eksportprofil

Den første SAF-T-slice er landet for ledger + salgsfakturaer. Det er bevidst smalt. Næste arbejde er at udvide eksporten uden at foregive fuld compliance før hver profilkant er testet:

- bredere dækning af de relevante tabeller/felter omkring køb, moms og dokumentreference
- tydelig profil/versionering så eksporten ikke ligner en generisk "færdig SAF-T"
- regression-tests der holder XML, manifest og audit-event deterministiske

Det her er ikke pynt. Eksporten er en del af projektets troværdighed over for myndigheder, revisorer og agenter.

### Asymmetric backup signatures — udvidelser

Ed25519-signering er i kernen. Ergonomi mangler: nem nøgle-rotation, public-key-distribution til revisor (QR-kode? PEM via mail?), uafhængig verify-CLI brugbar af nogen uden Rentemester installeret.

---

## Parking lot (senere, eller måske aldrig)

Det her er ting der har værdi men ikke har prioritet før kerne-brugersituationen er solid:

- **Encryption at rest** for SQLite-filen (relevant ved cloud-hosting; ikke ved lokal disk med fuld disk-encryption).
- **TastSelv API-integration** når Skat åbner en — i dag eksporterer Rentemester rubrik-tallene; integration vil gøre indberetning til ét klik.
- **Forventet skat / forskudsregistrering** ud fra periodens resultatopgørelse.
- **Mobile app** (læse-mest: status, godkend exceptions).
- **Lager / projektregnskab / løn** — det her er ikke ERP og skal ikke være det. Hvis du har brug for det, er Rentemester ikke det rigtige værktøj.
- **Multi-tenant SaaS-hosting** — kerneproduktet forbliver self-hostet open source. Hvis hosting opstår senere, er det som en separat tjeneste oven på, ikke en pivot af projektet.

---

## Hvad vi eksplicit IKKE bygger

- **Lock-in.** Dine data forbliver i en mappe du ejer. Eksport er en kerne-feature, ikke en betalt opgradering.
- **Abonnement på selve softwaren.** MIT-licens, ingen CLA-tricks. Eventuel betaling i fremtiden vil være for hosting eller support — aldrig for koden.
- **Chatbot-grænseflade.** Agenten er en CLI-/MCP-aktør, ikke en figur du taler til via boble-UI. Hvis du vil tale med en agent, er det din egen klient (Claude Desktop, Cursor, custom MCP-klient) — Rentemester er et værktøj agenten bruger.
- **Magic AI-bogføring uden audit-spor.** Reglerne afgør, ledgeren håndhæver. En agent må handle, men kan ikke kreativt fortolke regnskabet udenom triggers og validatorer.
- **Generisk regnskabssystem.** Vi bygger til danske mikrovirksomheder med relativt simple forhold. Ikke koncerner, ikke flere selskaber, ikke avanceret projektregnskab.

---

## Hvordan rækkefølgen ændrer sig

Tre ting kan rykke noget op:

1. **En kvalificeret bidragyder tager noget specifikt.** Hvis en bogholder vil review'e moms-fortolkningen, sker det først. Hvis en frontend-udvikler vil bygge web-UI, sker det først.
2. **Konkret bruger-friktion.** Hvis betatestere konstant snubler over manglende bilagsmail, rykker det op.
3. **Lov-ændringer.** Hvis bogføringsloven, momsloven, eller bekendtgørelse 2024-205 ændres, går det forrest. Vi citerer linje for linje — vi kan ikke være på fortidens version.

Roadmap'en er åben for debat. Issues > diskussion > prioritering > kode.
