![Rentemester banner](assets/rentemester-banner.png)

# Rentemester

**Bogholderen i maskinen — bygget til små danske virksomheder.**

Rentemester er et kommende bogholderisystem for danske mikrovirksomheder, freelancere, konsulenter og små ApS’er. Det kan bruges på to måder — en AI-agent kan drive det daglige bogholderarbejde for dig, eller du kan styre det selv direkte — mens selve regnskabet i begge tilfælde holdes på plads af faste regler, bilag, momslogik og en kontrollerbar historik.

Målet er enkelt:

> Du skal ikke bruge aftenen på at klikke rundt i et regnskabsprogram.  
> Systemet skal finde bilag, matche betalinger, bogføre det sikre og spørge dig om resten.

Rentemester er stadig under udvikling, men retningen er klar: **agent-first bogholderi med danske regler som fundament.**

---

## Hvem er Rentemester til?

Rentemester er tænkt til dig, der driver en mindre dansk virksomhed med relativt simple forhold:

- freelance- eller konsulentvirksomhed
- enkeltmandsvirksomhed
- lille ApS
- få ansatte eller ingen ansatte
- almindelige udgifter, bilag og fakturaer
- dansk moms
- behov for revisor- eller bogholder-eksport

Det er **ikke** tænkt som første version til store virksomheder, lager, løn, kasseapparat, avanceret projektregnskab eller komplekse internationale forhold.

---

## Hvad skal Rentemester kunne?

På sigt skal Rentemester kunne hjælpe med det praktiske bogholderi fra ende til ende:

- hente bilag fra en bilagsmail
- gemme bilag sikkert med dokumentation
- importere banktransaktioner
- matche banklinjer med bilag
- foreslå eller vælge korrekt konto og moms
- bogføre tydelige og sikre posteringer
- stoppe usikre posteringer og lave en opgaveliste
- oprette og sende fakturaer
- holde styr på åbne og betalte fakturaer
- lave momsrapport
- lave eksportpakke til revisor
- dokumentere hvad der er sket, hvornår og hvorfor

Den vigtige pointe er, at AI’en **ikke bare får lov til at gætte**.

AI-agenten kan gøre arbejdet, men Rentemesters kerne skal kontrollere, at bogføringen overholder reglerne.

---

## Grundideen

Traditionelle regnskabssystemer er ofte bygget sådan her:

```text
Du logger ind
→ finder bilag
→ vælger konti
→ afstemmer bank
→ laver moms
→ eksporterer til revisor
```

Rentemester vender modellen om:

```text
Systemet indsamler data
→ agenten udfører rutinearbejdet
→ reglerne kontrollerer alt
→ du håndterer kun undtagelserne
```

Med andre ord:

> **Agenten handler. Reglerne afgør. Ledgeren håndhæver.**

---

## To måder at bruge Rentemester på

Rentemester er bygget til at blive brugt på **to måder** — og begge er gyldige. Det er ikke to forskellige programmer; det er samme kerne, samme regler og samme ledger, betjent af enten en agent eller et menneske.

### 1. Agent-betjent

En selvstændig AI-agent driver bogholderiet for dig. Agenten læser bilag, foreslår posteringer og bogfører det entydige — alt sammen gennem Rentemesters værktøjer, ikke ved at gætte.

Det ser sådan ud i praksis:

- Du peger agenten på din virksomhed og en mappe med nye bilag.
- Agenten kører en fast bogføringsrunde: indlæser bilag, bogfører det sikre, lægger det usikre i en opgaveliste, afstemmer banken og tjekker moms- og regnskabsårsfrister.
- Til sidst får du en kort rapport over hvad der blev gjort, og hvad der venter på dig.

Teknisk sker det enten via kommandoen `agent run` (én komplet, gentagelig runde) eller via Rentemesters MCP-server, så en agent i fx en chatklient kan kalde de samme værktøjer skridt for skridt. Agenten gætter aldrig: alt den ikke kan afgøre med sikkerhed, bliver til en opgave på listen — aldrig en postering.

### 2. Menneske-betjent

Du driver det selv, direkte. Hver handling i Rentemester er en kommando på kommandolinjen — udsted en faktura, importér en bank-CSV, bogfør en udgift, kør momsrapporten, luk en periode.

Det ser sådan ud i praksis:

- Du kører kommandoer selv, ét skridt ad gangen, og ser resultatet med det samme.
- De samme regler og kontroller gælder: en postering der ikke balancerer, eller en faktura med fejl, bliver afvist — uanset hvem der kalder.
- Du kan til enhver tid bede en agent overtage en del af arbejdet, eller selv overtage fra agenten. Historikken er den samme uanset hvem der handlede.

Begge måder skriver i samme ledger, følger samme regler og efterlader samme reviderbare spor. Forskellen er kun, hvem der trykker på knapperne.

---

## Hvorfor ikke bare “AI i et regnskabsprogram”?

Fordi bogføring kræver tillid.

En chatbot må gerne være kreativ. Et bogholdersystem må ikke være kreativt med dit regnskab.

Rentemester bygges derfor med en hård kerne:

- dobbelt bogholderi
- debet skal være lig kredit
- bilag skal gemmes og kunne spores
- bogførte posteringer må ikke bare ændres
- fejl skal rettes med nye posteringer
- moms skal beregnes efter tydelige regler
- fakturaer skal valideres før de udstedes
- alle handlinger skal kunne revideres

AI’en må hjælpe. Systemet skal sige nej, når noget ikke er sikkert nok.

---

## Eksempel på en hverdag med Rentemester

Forestil dig en typisk måned:

1. Du sender eller videresender bilag til en bilagsmail.
2. Rentemester importerer bankbevægelser.
3. Systemet matcher f.eks. Stripe, Google, OpenAI, DSB og kontorudgifter med bilag.
4. Klare posteringer bogføres automatisk.
5. Usikre ting kommer i en kort opgaveliste:
   - “Restaurantbilag mangler formål og deltagere”
   - “Bilag mangler for kortbetaling på 1.250 kr.”
   - “Leverandør fra EU kræver reverse charge-vurdering”
6. Du svarer kun på det, der kræver menneskelig viden.
7. Ved momsperiodens slutning får du en rapport og eksport til revisor.

Målet er ikke at fjerne ansvar. Målet er at fjerne gentagelser, rod og unødige klik.

---

## Cockpittet — den grafiske brugerflade

Rentemester har et **lokalt web-baseret kontrolpanel** — *cockpittet* — der ligger oven på samme ledger, regler og API som CLI'en. Hvis du ikke har lyst til at leve i terminalen, er det her du skal være.

Cockpittet giver dig en visuel hverdag:

- **Overblik** med likviditet, åbne fakturaer, exceptions og næste momsfrist
- **Resultatopgørelse, balance, saldobalance** og flerårig sammenligning
- **Bilag** — søg, filtrér og åbn dokumenter direkte
- **Bank** — importér CSV, se transaktioner og match mod bilag/fakturaer
- **Posteringer (journal)** og **forpligtelser** (moms, A-skat, frister)
- **Fakturaer** — udsted, send på e-mail, registrér rykker/betaling, kreditnota
- **Kontakter** — kunder og leverandører med CVR-opslag
- **Hjælp** — indbygget guide til de almindelige flows

### Sådan starter du cockpittet

I én terminal — start API'en (samme kerne som CLI'en bruger):

```bash
bun run src/cli.ts serve --workspace ./min-workspace
# Cockpit backend listening on http://127.0.0.1:4319
```

I en anden terminal — start cockpit-UI'en:

```bash
cd app && bun install && bun run dev
# Vite-dev-server på http://localhost:5319
```

Åbn `http://localhost:5319` i en browser. Vite proxer `/api`-kald videre til backend på port 4319, så cockpittet og CLI'en altid arbejder på den samme ledger.

> Alt cockpittet kan, kan CLI'en også — og omvendt. Samme regler, samme audit-trail, samme append-only historik. Cockpittet er en blødere indgang; CLI'en er en hårdere én.

---

## Se det virke

Et eksekverbart eksempel ligger i [`examples/agent-demo/`](examples/agent-demo/) — det viser én månedes bogføring fra ende til ende over Rentemesters MCP-overflade.

```bash
bun run agent-demo
# eller direkte:
bun examples/agent-demo/run.ts --company /tmp/agent-demo --mode rule-based
```

Demoen importerer en bank-CSV, ingester 6 bilag (Google Workspace, OpenAI, AWS, DSB, Elgiganten, og en restaurant-bon uden formålsbeskrivelse), foreslår match mod bankudtog, auto-bogfører 5 høj-confidence udgifter, og lader resten ligge i exception-køen. Til sidst kører den momsrapport, audit-verifikation og healthcheck. Ingen API-keys, intet netværk — alt kører lokalt over MCP-stdio mod `src/mcp/server.ts`.

Se [`examples/agent-demo/README.md`](examples/agent-demo/README.md) for forventet output og kode-walkthrough.

---

## Hvad er bygget nu?

Rentemester er stadig under udvikling og bør endnu ikke være din eneste kilde til sandhed (se forbeholdet længere nede). Men kernen virker allerede — alt nedenfor kan køres i dag, både af en agent og af et menneske. Listen er grupperet, så du som virksomhedsejer kan se hvad det betyder for dig.

**Sådan kommer data ind**

- importér banktransaktioner fra en CSV-fil, også fra udenlandske konti — kursen og DKK-beløbet gemmes
- indlæs bilag (PDF, billeder, kassestrimler) med dokumentation; også fysiske udenlandske bilag i original valuta
- hent bilag fra en mappe med e-mails (`.eml`-filer / maildrop) — første trin mod en rigtig bilagsmail
- hent bilag direkte fra en e-mailkonto via IMAP, så indbakken tømmes automatisk
- slå dine egne og dine kunders/leverandørers stamdata op i CVR-registret og udfyld kunde- og leverandørkartoteket automatisk
- importér en kundeliste fra et andet system og migrér en hel virksomhed ind via en åbningsbalance

**Bogføring og daglig drift**

- en fungerende kassebog (ledger) i en lokal SQLite-fil med dansk kontoplan
- bogfør udgifter direkte fra bilag + bankpost, også i fremmed valuta når banken trækker DKK
- bogfør manuelle posteringer med krav om at debet er lig kredit
- afstem banken og få deterministiske forslag til hvilke bankposter der hører til hvilke bilag og fakturaer
- en opgaveliste (exceptions) hvor alt usikkert havner — så det kan løses bevidst i stedet for at blive gættet
- ret fejl ved at tilbageføre med en ny postering — aldrig ved at slette
- luk regnskabsperioder, så lukkede og fremtidige perioder ikke kan bogføres ved en fejl
- kørselsregnskab og register over anlægsaktiver med afskrivning over tid

**Fakturering**

- validér og udsted danske fakturaer med moms og forfald; et immutabelt snapshot gemmes
- generér en PDF af en udstedt faktura
- send en faktura eller en betalingspåmindelse til kunden på e-mail med PDF'en vedhæftet — afsendelsen logges og kan ikke ske dobbelt ved et uheld. *Bemærk:* selve den indbyggede e-mailafsendelse kører foreløbig i test-tilstand; rigtig levering kræver at man tilkobler sin egen e-mailkanal
- gentagne fakturaer via skabeloner (månedlige, kvartalsvise, årlige) — første trin mod abonnementsfakturering
- registrér betalinger og afstem dem mod banken
- udsted kreditnotaer og bogfør refundering tilbage til kunden
- automatisk bogføring af fakturaer til debitorer, omsætning og salgsmoms
- følg forfald, og beregn og bogfør lovbestemt morarente, rykkergebyrer og kompensationsbeløb ved for sen betaling
- afskriv tab på en kunde der ikke betaler, med korrekt momsregulering
- forbered e-faktura til offentlige kunder: EAN/GLN-eksport og et gyldigt Peppol BIS Billing 3.0-dokument, klar til forsendelse via et access point

**Moms og rapporter**

- momsrapport for en periode og en indberetningsklar momsangivelse med SKAT-rubrikker
- EU-servicekøb med reverse charge og repræsentationsudgifter med korrekt delvis momsfradrag
- validér EU-momsnumre mod VIES før EU-bogføring
- regnskabsrapporter: saldobalance, resultatopgørelse og balance
- en årsrapport for regnskabsklasse B, der kan skrive en iXBRL-fil — Rentemester forbereder, du og din revisor gennemgår og indberetter
- et statisk HTML-dashboard over virksomhedens aktuelle status

**Sikkerhed, backup og udlevering**

- append-only historik med en hash-kæde, så manipulation kan opdages
- valgfri kryptografisk signering (ed25519), så en tredjepart kan verificere uafhængigt
- en `audit verify`-kommando der tjekker hele kæden
- revisionsklare backups, der kan pakkes til ét arkiv og lægges et sikkert sted, med en backup-styring der holder øje med den ugentlige backup-pligt, attesterede destinationer i EU/EØS og en frivillig bogførings-lås
- eksportpakker til myndighedsudlevering, til en kurator og en lokal håndoff-pakke til din bogholder eller revisor — fil-overdragelse, ikke live adgang
- en første SAF-T-eksport efter den nye bogføringslov
- GDPR-værktøjer: saml alle persondata om en kunde i én indsigtsrapport, og slet det der ikke længere skal opbevares — bogføringspligten går altid forud for sletteret

**Brug og drift**

- alt kan køres fra kommandolinjen (menneske-betjent) og via MCP-værktøjer (agent-betjent)
- en runtime-agent (`agent run`), der kører én komplet, gentagelig bogføringsrunde på en allerede `init`-initialiseret virksomhedsmappe
- danske retskilder downloades og hash-verificeres, så regler kan spores til en kilde
- automatiske tests, og en container til drift med din virksomhedsmappe monteret

Det er ikke færdigt, men fundamentet er lagt rigtigt: regler først, dokumentation først, audit først.

---

## Hvad mangler stadig?

Meget virker allerede, men før Rentemester kan bruges som dit rigtige bogholderisystem mangler der bl.a.:

- bedre automatisk match mellem bank og bilag (forslagene findes, men kan blive skarpere)
- rigtig levering af fakturaer på e-mail ud af boksen — i dag bygges e-mailen, men selve afsendelsen kører i test-tilstand, indtil en e-mailkanal er tilkoblet
- direkte PEPPOL-transport til offentlige kunder, ikke kun forberedte forsendelsesdokumenter
- bredere SAF-T-dækning oven på den første afgrænsede eksport
- adgang for din bogholder eller revisor direkte i et hosted system med rollegrænser — i dag er det en lokal fil-håndoff
- en interaktiv brugerflade oven på det statiske dashboard
- direkte bankfeeds (PSD2/open banking) og integrationer til fx PayPal, Zettle og Shopify
- et mere komplet dansk regelbibliotek
- grundig gennemgang sammen med en bogholder eller revisor

Rentemester skal ikke kaldes færdigt, før det kan tåle at være source of truth.

---

## Danske regler og kilder

Rentemester bygges med danske regler som udgangspunkt.

Projektet samler og versionsstyrer relevante kilder som bl.a.:

- Bogføringsloven
- krav til digitale bogføringssystemer
- Momsloven
- Momsbekendtgørelsen
- regler om bilag, opbevaring og fakturaer

Regler skal ikke bare beskrives i tekst. De skal gøres testbare:

- Hvad kræver reglen?
- Hvornår gælder den?
- Hvad skal systemet afvise?
- Hvilken kilde bygger det på?
- Findes der en test, der beviser det?

Det er den langsomme vej. Men det er den rigtige vej.

---

## Open source og ingen lock-in

Rentemester bygges som open source.

Principperne er:

- dine regnskabsdata skal være dine
- systemet skal kunne køre lokalt eller på egen server
- data skal kunne eksporteres
- bilag skal gemmes i almindelige mapper
- der skal være audit trail
- systemet må ikke låse dig inde

En lille virksomhed skal ikke miste adgang til sit regnskab, fordi et abonnement, API eller eksternt system ændrer sig.

---

## Vigtigt forbehold

Rentemester er **ikke** en revisor, bogholder eller juridisk rådgiver.

Projektet er under udvikling og må ikke bruges ukritisk til rigtig bogføring endnu. Brug altid sund fornuft, og få hjælp fra en bogholder eller revisor, når det gælder moms, skat, årsregnskab og tvivlsspørgsmål.

Ambitionen er, at Rentemester på sigt kan blive et pålideligt værktøj — men tillid skal fortjenes med regler, tests, dokumentation og praktisk brug.

---

## Kort sagt

Rentemester er et forsøg på at bygge bogholderi til en ny virkelighed:

```text
Agenten er bogholderen.
Ledgeren er loven.
Reglerne er kontrakten.
Bilagene er beviserne.
Dashboardet er kontrolrummet.
```

Et regnskabssystem bygget til AI-agenter — men med danske regler, bilag og revision i centrum.
