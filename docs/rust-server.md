# Kør Rust-serveren med web-UI'et

Rust-serveren leverer det eksisterende React-cockpit og behandler selv login,
ejeroprettelse, MFA, sessions, virksomhedsadgang, manuel bogføring og saldobalance. Den starter
ikke TypeScript-backenden og sender ikke forespørgsler videre til den.

## Prøv med syntetiske data

Fra repository-roden, med Docker og Bun samt projektets afhængigheder installeret:

```sh
python3 scripts/rust/run-web-demo.py
```

Launcheren bygger cockpittet og Rust-serveren, opretter et isoleret workspace
med en eksplicit syntetisk kontoplan og viser et privat installationslink.
Åbn installationslinket og opret din demo-ejer i browseren. Launcheren viser også
kommandoen til at læse det lokale bekræftelseslink efter oprettelsen.

I browseren:

1. Opret den første ejer via installationslinket, og åbn e-mailbekræftelseslinket.
2. Log ind og opsæt totrinsbekræftelse. Gem recovery codes.
3. Vælg virksomheden med **Åbn bogføring**.
4. Vælg **Ny postering**, indtast tekst og beløb, og vælg debet- og kreditkonto.
5. Gennemse og bekræft posteringen.
6. Åbn **Saldobalance** og kontrollér debet, kredit og saldo.

Demoens to konti bruges til en manuel åbningspostering. Indtægter og udgifter
kræver fortsat bilag; domænekontrollen er ikke lempet for demoen.

Ctrl-C stopper demoen og fjerner dens container og datavolume. Kun en separat
Rust-buildcache bevares. Eksisterende virksomhedsdata bruges ikke.

## Kontroller hele forløbet

Med Playwrights Chromium installeret:

```sh
python3 scripts/rust/run-web-demo.py --check
```

Kontrollen bruger en rigtig browser og den rigtige Rust-server uden mockede
API-svar. Den opretter første ejer i browseren, afviser ugyldige og allerede
brugte installationslinks, gennemfører login og MFA, bogfører 125,50 kr., kontrollerer
saldobalancen, genstarter serveren og logger ind igen med en recovery code.
Den kontrollerer derefter den bevarede postering og saldobalance samt, at
samme idempotente bogføringskald ikke opretter en dublet. Et skærmbillede gemmes
som `/tmp/rentemester-rust-first-journey.png`. Testdata fjernes efter kørslen.

## Aktuel afgrænsning

Launcheren opretter workspacets filer lokalt. Første ejer oprettes i browseren
via et privat installationslink, som den betroede lokale `setupActor` aktiverer
fra serverkonfigurationen. Linket gælder en time og lukkes efter vellykket
opsætning. Servergenstart kontrollerer den gemte status og genåbner ikke en
afsluttet opsætning. Bekræftelseslinks leveres som private lokale filer, ikke
som eksterne e-mails.

Workspace-ejerskabet bruger foreløbig den eksisterende beskyttede Linux-profil;
på macOS kører demoen derfor i Docker. Serverens `--config` bruges med native
workspaces og en betroet lokal konfiguration; `--create` opretter et nyt workspace,
og uden flaget genåbnes det eksisterende. `--origin` angiver browserens lokale
origin, når serveren kører bag Docker-portmapping.

Rust-cockpittets startside viser foreløbig virksomhedsvælgeren. Resten af
porteføljeoverblikket, øvrige bogføringsforløb, eksport, desktop og produktionsdrift
er fortsat under omskrivning. Denne demo er ikke den færdige Rust-migration.
