# Revisionshistorik på Rust-serveren

I **Integritet → Revisionsspor** kan brugeren hente virksomhedens registrerede
hændelser. Filtre for fra/til-dato, hændelse og aktør kombineres. Et nyt
filteropslag starter fra første side. Sideskift bruger de indsendte filtre,
selv om formularen er blevet redigeret i mellemtiden. Afviste opslag fjerner
et tidligere resultat.

Hver række viser det oprindelige id, tidspunkt, aktør, hændelse, reference og
besked. Beskeder vises som almindelig tekst. Listen ændrer ingen posteringer.
Den er en historikvisning; fuld `audit_verify` er fortsat en særskilt portering.

## HTTP, CLI og MCP

`GET /api/companies/{slug}/audit-log` accepterer disse valgfrie queryfelter:

| Felt | Betydning |
| --- | --- |
| `limit` | 1–5000, standard 500 |
| `offset` | Heltal fra 0 til 9007199254740991, standard 0 |
| `fromDate`, `toDate` | Gyldig `YYYY-MM-DD` eller RFC3339 med eksplicit tidszone |
| `eventTypeLike`, `actorLike` | Højst 256 tegn, ingen kontroltegn |

En bar fra-dato begynder ved UTC-midnat; en bar til-dato medtager hele
UTC-dagen. Tidsstempler normaliseres til UTC og sammenlignes med SQLites
oprindelige tidspræcision. Returnerede tidspunkter ændres ikke. Tekstfiltre
bruger samme case-insensitive SQL LIKE-søgning som den tidligere motor;
`%` og `_` fungerer som jokertegn. Alle filtre kombineres med AND.

Ukendte eller gentagne queryfelter, ugyldige tal og omvendte datointervaller
afvises. Querystrengen må højst være 4096 bytes, og GET accepterer ingen body.
En succesfuld side må højst være 8 MiB; et større resultat afvises med
`audit_log_page_too_large`. Bed om færre rækker frem for at afkorte beskeder.

Svaret indeholder `ok`, `slug`, `rows`, `total`, `count`, `limit`, `offset`,
`hasMore` og kun ved flere rækker `nextOffset`. Rækker sorteres efter det gemte
`created_at DESC, id DESC`. Optælling og side læses i samme transaktion.
Forskellige sideskift er nye observationer: nye hændelser kan flytte rækker.
Ingen klient henter automatisk alle sider eller præsenterer afkortning som
et fuldstændigt resultat.

SDK bruger `ApiClient::audit_log(company, &AuditLogQuery)`. CLI bruger
`remote audit-log-list --company SLUG` med `--limit`, `--offset`, `--from-date`,
`--to-date`, `--event-type-like` og `--actor-like`. MCP bruger `audit_log_list`
med de tilsvarende camelCase-felter. De returnerer serverens samme side.

Aktuel `CompanyRead` kræves: normal MFA-cookie eller en eksplicit servicekonto
med gældende scope og virksomhedstilknytning. Serviceopslag følger eksisterende
kvoter. Tilbagekaldt session, nøgle eller medlemskab giver ikke adgang til gamle
resultater. Cookie og servicenøgle må ikke kombineres. Klienten accepterer ingen
klientvalgt aktør som autoritet.

## Verificeret brugerforløb

Den syntetiske acceptprøve bogfører og tilbagefører én postering og finder de
præcise hændelser via web, HTTP, CLI og MCP. Den sammenligner hele historikken
med offset-sider, aktør/type/dato-filtre og serviceopslag, afviser ugyldige kald,
og giver en læser adgang før den tilbagekaldes. Bogføringen ændres ikke af
opslagene. Historik og aktører/tidspunkter bevares efter normalt MFA-login og
genstart. Den installerede desktopprøve udføres til sidst.
