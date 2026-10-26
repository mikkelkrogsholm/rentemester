# Bankimport: kanonisk idempotenskontrakt

Denne kontrakt er afledt direkte af `transactionFingerprint` i
[`src/core/bank.ts`](../src/core/bank.ts). Den gælder både CLI `bank import`
og MCP `bank_import`.

## Fingerprint og kontoskop

En importeret række identificeres af SHA-256 over disse normaliserede felter:

- `bank_account_id` — samme bankpost på to forskellige bankkonti er ikke en dublet.
- `transaction_date`
- `booking_date` (`null`, når den mangler)
- trimmet `text`
- afrundet `amount`
- normaliseret `currency`
- `reference` (`null`, når den mangler)
- afrundet `amount_dkk` (`null`, når den mangler)
- `fx_rate_to_dkk`, afrundet til seks decimaler (`null`, når den mangler)
- `occurrence`

`sourceFileHash` hører til importbatchens proveniens/auditspor; det er ikke
den nøgle, der afgør om en bankrække er en dublet.

## Re-importer og ens rækker

`occurrence` er en nulbaseret tæller for tidligere rækker med samme indhold i
**den aktuelle CSV-fil**. Derfor gælder følgende:

- En fuld re-import af samme CSV springer alle allerede importerede rækker over.
- En delvis eller overlappende re-import springer de rækker over, hvis samme
  indhold og occurrence allerede findes; nye rækker importeres.
- To legitimt ens rækker i samme CSV (for eksempel to gebyrer på 50 kr. samme
  dato) får occurrence `0` og `1` og importeres begge. En senere re-import af
  den samme fil springer begge over.

Begrænsning: `occurrence` tælles kun inden for den senere importfil. Hvis en
senere fil kun indeholder den anden af to ellers identiske rækker fra en
tidligere fil, får den igen occurrence `0`; den matcher derfor den tidligere
første række og springes over. Importér den komplette, sammenhængende eksport
for at bevare rækkefølgen af identiske transaktioner.

Ændres et fingerprint-felt — også bankkonto, bookingdato, reference,
`amount_dkk` eller FX-kurs — er det en anden række for importens dedup-regel.
Kontrollér derfor bankeksporten før du ændrer data og re-importerer.

## Kildeorden og faktisk banksaldo

En løbende `balance_after` er kun autoritativ, når Rentemester kan bevise den
sidste kildelinje. En importprofil kan erklære rækkefølgen, og en generisk CSV
kan gøre det eksplicit med `--statement-order ascending|descending` (eller
`statementOrder` i MCP/HTTP). Retningen og rækkeindekset gemmes append-only på
hver importeret bankrække.

Ved flere rækker på samme dato bruger saldoresolveren denne lagrede kildeorden
— aldrig SQLite-id eller importtidspunkt. Proveniensen er bundet til den
immutable `sourceFileHash` plus den erklærede profil/retning; to forskellige
eksporter på samme dato kan derfor ikke stiltiende kædes sammen. En manglende, modstridende eller
inkonsistent saldo-kæde giver status `ambiguous` og ingen faktisk banksaldo.
Det samme fail-closed resultat og den tilhørende proveniens bruges af
portefølje, overblik, bank- og likviditetsvisninger samt API'et.
