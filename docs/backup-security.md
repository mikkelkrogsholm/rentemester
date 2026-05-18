# Backup-signing chain-of-trust (audit af issue #87)

Dette dokument beskriver hvor signing-nøglen til `system backup` ligger,
hvad den beskytter, og hvad den **ikke** beskytter. Det er resultatet af
en research-loop bestilt af issue
[#87](https://github.com/mikkelkrogsholm/rentemester/issues/87) som
opfølgning på den oprindelige signing-implementering i issue
[#33](https://github.com/mikkelkrogsholm/rentemester/issues/33).

Alle citater er fra `main`-træet på audit-tidspunktet (2026-05).

## Genereringspunkt

Nøglen genereres lazy af `ensureBackupManifestKey()` i
[`src/core/system-backups.ts`](../src/core/system-backups.ts) (linje 89-95):

```ts
function ensureBackupManifestKey(companyRoot: string) {
  const existing = readBackupManifestKey(companyRoot);
  if (existing) return existing;
  const key = randomBytes(32);
  writeFileSync(backupManifestKeyPath(companyRoot), `${key.toString("hex")}\n`, { mode: 0o600 });
  return key;
}
```

- **Hvornår**: Første gang `createSystemBackup()` køres for en
  virksomhed. `ensureBackupManifestKey` kaldes inde i selve backup-flowet
  (linje 228).
- **Hvad**: 32 bytes (256 bit) fra Node's CSPRNG `crypto.randomBytes`,
  skrevet hex-encoded med trailing newline.
- **Filtilladelser**: `0o600` (læs/skriv for ejer, intet for andre).
- **Algoritme**: HMAC-SHA256 (symmetrisk). Signaturen er
  `HMAC_SHA256(key, manifest.json)` udregnet i `signManifestText()`
  (linje 101-103) og verificeret med `timingSafeEqual()` i
  `verifyManifestAuthenticity()` i
  [`src/core/system-restore.ts`](../src/core/system-restore.ts) linje 89-105.

## Lagringssti

Stien er defineret af `backupManifestKeyPath()` i `system-backups.ts`
linje 73-75:

```ts
export function backupManifestKeyPath(companyRoot: string) {
  return join(companyRoot, ".backup-manifest.key");
}
```

- **Absolut eksempel** (smoke-flow): `/tmp/rentemester-smoke/.backup-manifest.key`
- **Relativ til company-root**: `./.backup-manifest.key`
- **Vigtigt**: Nøglen ligger **i company-root**, ikke i `config/`,
  `data/` eller `backups/`. Det betyder den ikke ender i selve backup-tar'en
  (se næste afsnit).

## Inkluderet i backup?

**Nej.** Backup-flowet kopierer kun tre nedstrøms-mapper og en
DB-snapshot:

```ts
copiedFiles: {
  documentsOriginals: copyDirWithManifest(paths.documentsOriginals, documentsBackupDir, backupDir),
  invoicesIssued:     copyDirWithManifest(paths.invoicesIssued,     invoicesBackupDir,  backupDir),
  config:             copyDirWithManifest(paths.config,             configBackupDir,    backupDir),
},
```

`companyPaths()` ([`src/core/paths.ts`](../src/core/paths.ts) linje 6-22)
mapper disse til `<root>/documents/originals`, `<root>/invoices/issued` og
`<root>/config`. Nøglefilen `<root>/.backup-manifest.key` er ikke en del
af nogen af de tre kildemapper, så den kopieres aldrig.

Empirisk verifikation efter `bun run smoke`:

```text
$ find /tmp/rentemester-smoke/backups -name "*.key*"
(tom)
$ find /tmp/rentemester-smoke -name "*.key*"
/tmp/rentemester-smoke/.backup-manifest.key
```

Dette er låst af regressionen i
[`tests/unit/backup-security.test.ts`](../tests/unit/backup-security.test.ts).

## Rotation

**Der er ingen `system rotate-backup-key`-CLI** på audit-tidspunktet.
Suchen viser nul forekomster af `rotate`/`rotation` i `src/`.

Manuel procedure hvis nøglen kompromitteres:

1. Slet `<companyRoot>/.backup-manifest.key`.
2. Kør `bun run src/cli.ts system backup --company <root> --at <iso>`.
   En ny tilfældig nøgle genereres af `ensureBackupManifestKey()`.
3. **Konsekvens**: Alle eksisterende backups bliver
   uverificerbare med den nye nøgle — `keyHint` ændrer sig, og
   `verifyManifestAuthenticity()` vil afvise restore. Gem den gamle
   nøgle separat hvis ældre backups stadig skal kunne verificeres,
   eller lav en frisk fuld backup straks efter rotation.

Mangel på CLI-rotation er noteret som ergonomi-forbedring i
"Anbefalede forbedringer" nedenfor.

## Public-key-eksport

**Nu muligt via opt-in ed25519 (issue #99).** HMAC-SHA256 forbliver
default for hurtig tamper-detection — den er rotation-let og kræver
ingen ny mekanik. For 3.-parts-verifikation tilbydes en parallel
asymmetrisk signatur:

- `system backup --sign-with-ed25519` genererer ved første kald et
  ed25519-keypair (`crypto.generateKeyPairSync("ed25519")`) og skriver
  manifest-signaturen som `manifest.json.ed25519.sig` (base64-encoded
  64-byte ed25519-signatur).
- **Private key** ligger på `<companyRoot>/.backup-signing-key.pem`
  (PKCS#8 PEM, mode `0o600`). Den kopieres aldrig ind i backuppen — på
  samme måde som HMAC-nøglen.
- **Public key** ligger på `<companyRoot>/config/backup-manifest.pub`
  (SPKI PEM). Den ligger inde i `config/` og ryger derfor automatisk med
  i hver backup via den almindelige config-kopi, så enhver der har
  backuppen også har den public key der svarer til signaturen.
- `system export-public-key --company <root> --out <file>` kopierer
  public key ud til en separat fil, som virksomheden kan sende til
  revisor/Skattestyrelsen out-of-band (signet email, attesteret
  papirkopi, etc.) før selve backuppen overdrages.
- `system verify-backup-signature --backup-dir <dir> --public-key <pem>`
  verificerer en backup standalone uden restore og uden adgang til
  source-company-root. Bruges af 3.-parten.
- `system restore-backup --public-key <pem>` accepterer også eksplicit
  public key hvis verifikatoren ikke stoler på den der ligger i
  backuppens config/.

Når en backup er signeret med ed25519, **skal begge signaturer**
(HMAC + ed25519) verificere ved restore. Opt-in svækker aldrig HMAC.

For HMAC alene gælder stadig:

- `restoreSystemBackup()` tager en `verificationKeyPath` (flag
  `--verify-key`) der peger på den symmetriske hemmelighed. Brug kun
  denne i en flow hvor verifikatoren reelt er virksomheden selv.
- Non-repudiation findes stadig kun via ed25519 — HMAC-signaturen kan
  pr. definition forfalskes af alle der har nøglen.

## Trusselsmodel

Scenarierne nedenfor antager en angriber **uden for** virksomhedens
operationelle perimeter. "Source company-root" betyder hele
`<companyRoot>/`-mappen inklusiv `.backup-manifest.key`.

| # | Scenarie | Authenticity (kan signaturen forfalskes?) | Confidentiality (kan data læses?) | Verdict |
|---|---|---|---|---|
| 1 | Angriber har **læseadgang** til `<root>/backups/` (fx fejlkonfigureret cloud-sync, lækket disk-snapshot) | **Beskyttet** — nøglen ligger ikke i backup-mappen, så ingen forgery mulig | **Ikke beskyttet** — manifest, ledger.sqlite, dokumenter og fakturaer ligger i klartekst i backup-mappen | Tamper-evidence intakt; data eksponeret. Brugeren skal selv kryptere backup-mappen ved off-site replikation. |
| 2 | Angriber har **skriveadgang** til `<root>/backups/` men **ikke** til `<root>/.backup-manifest.key` (fx læk af staging-bucket-credentials der kun har skriveret til backup-prefix) | **Beskyttet** — `verifyManifestAuthenticity()` afviser ændret manifest. Faktiske filændringer fanges af sha256 i manifest under `ensureMatches()` (system-restore.ts:67-76) | **Ikke relevant** (samme som #1 hvis der også er læseadgang) | Tamper-evidence virker. Restore vil fejle, hvilket er det ønskede signal. |
| 3 | Angriber har **fuld adgang til source company-root** før backup tages (RCE på serveren der kører `rentemester`) | **Ikke beskyttet** — angriberen har både nøgle og indhold og kan producere arbitrære signed backups | **Ikke beskyttet** — samme reason | Uden for backup-systemets ansvar. Mitigeres af host-hardening, ikke af signing. |
| 4 | Backup overdrages til **3.-part** (revisor/Skattestyrelsen) der vil verificere **uden** source-adgang og uden at risikere at få forge-evne | **Beskyttet** når backup er taget med `--sign-with-ed25519` (issue #99). 3.-part modtager kun public key, kan verificere men ikke signere. Med HMAC alene gælder den oprindelige begrænsning: ingen 3.-parts-verifikation uden forge-evne. | n/a | **Løst for ed25519-stien.** HMAC-only-backups understøtter stadig ikke scenarie #4. |
| 5 | Angriber bytter en **gammel ægte backup** ud med en endnu ældre ægte backup (rollback) | **Ikke beskyttet** mod selve manifestet (gammel signatur er stadig gyldig), men `system backup-status` viser `latestBackupAt` baseret på manifestets `createdAt` — rollback synligt for operatøren der overvåger | n/a | Delvist beskyttet ved observation. Append-only ledger-log fanger ikke dette; kun out-of-band bookkeeping af backup-historik gør. |

## Sårbarheder fundet

1. **Ingen asymmetrisk signering — 3.-part kan ikke verificere uden forge-evne.**
   ~~Opfølgningsissue:~~
   [#99](https://github.com/mikkelkrogsholm/rentemester/issues/99)
   "Support asymmetric backup signatures for 3rd-party verification".
   **Status: lukket.** Ed25519 er nu tilgængelig som opt-in via
   `system backup --sign-with-ed25519`; revisor får public key via
   `system export-public-key` og verificerer standalone via
   `system verify-backup-signature`. HMAC bevares som default, og
   eksisterende HMAC-only-backups er fuldt bagudkompatible.

## End-to-end-flow: revisor-verifikation med ed25519

```bash
# 1) Virksomhed laver en signeret backup
bun run src/cli.ts system backup \
  --company /path/to/company \
  --at 2026-05-17T02:09:00.000Z \
  --sign-with-ed25519

# 2) Virksomhed eksporterer public key og sender den til revisor
#    out-of-band (signet email, attesteret papirkopi, ...).
bun run src/cli.ts system export-public-key \
  --company /path/to/company \
  --out /tmp/handover/company.pub

# 3) Backup-mappen overdrages til revisor (USB, tar.gz, hvad som helst).

# 4) Revisor verificerer standalone — uden adgang til source-company-root
#    og uden HMAC-nøglen.
bun run src/cli.ts system verify-backup-signature \
  --backup-dir /path/to/backup-20260517T020900Z \
  --public-key /tmp/handover/company.pub
# -> { "ok": true, "algorithms": ["ed25519"], ... }
```

Det er konstruktionen der gør scenarie #4 i tabellen ovenfor til
"Beskyttet": revisor får verifikationsevne, ikke forge-evne.

## Anbefalede forbedringer (ikke-blokerende)

- **Tilføj `system rotate-backup-key`-CLI** der atomisk laver: ny key →
  re-sign af eksisterende manifests med en `keyHint`-allowlist, eller
  alternativt en frisk fuld backup. Aktuelt skal operatøren manuelt
  slette key-filen.
- **Lås nøgle-stien via en regression-test** så fremtidige refactors
  ikke flytter nøglen ind i `config/` (hvor den ville havne i backup).
  Det dækkes nu af `tests/unit/backup-security.test.ts`.
- **Logg `keyHint` i audit-trail** ved hver backup, så rotation
  bagudrettet kan ses i `audit_log`. I dag logges kun
  `Created full backup <backupId>`.

## Konklusion

Signing-chainen er **bevisstærk for tamper-detection** så længe
nøglefilen ikke kompromitteres sammen med backup-mappen. Den giver
**ikke** confidentiality. Den giver independent third-party
verification **når backuppen er taget med `--sign-with-ed25519`**
(issue #99); HMAC-only-backups understøtter stadig ikke 3.-parts
verifikation uden forge-evne. Issue #87 blev lukket med dette dokument;
issue #99 lukkes med tilføjelsen af ed25519-stien.
