#!/bin/sh
/bin/cat <<'FOCUS'
Rentemester — hold fokus på brugerens mål:
- Færdiggør Rust-omskrivningen: én fælles motor/server og samme React-UI i web og Rust/Tauri-desktop. Manuel desktopaccept ligger til allersidst.
- Vælg næste nødvendige, hele brugerforløb ud fra eksisterende GitHub-issues. Hovedagenten ejer prioritering og accept; delegér kun uafhængige leverancer med klart filansvar.
- Hver ændring skal opfylde et konkret acceptkriterium eller fjerne en påvist blokering. Parkér valgfri polish, spekulativ hardening og ekstra infrastruktur.
- Brug den mindste tilstrækkelige test og eksisterende fixtures. Genbrug gyldig evidens; gentag kun ved ændret kode, konkret fejl eller ny viden.
- Hvis testopsætning tager over 15 minutter eller fejler to gange: STOP den voksende opsætningsrunde og revurdér fremgangsmåden. Isolér fejlen, vælg et enklere gyldigt bevis, eller registrér præcist det manglende bevis og fortsæt uafhængigt produktarbejde. Start ikke endnu en dyr prøve på samme uafklarede grundlag.
- Svæk aldrig nødvendige regnskabs-, adgangs- eller datakontroller for at få grønt. Manglende bevis er ikke færdigt.
- Når accept består: integrér og gå videre. Ingen ekstra perfektioneringsrunde. Rapportér færdige brugerforløb og konkret restarbejde.
FOCUS
