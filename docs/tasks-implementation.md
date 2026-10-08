# Opgaver: implementering og accept

Status: under implementering. Ingen acceptscenarier er endnu verificeret.

## Kontrakt

Ét workspace-ejet opgavegrundlag med selskabs- og workspace-scope, fælles
status og lokale boards. Liste, kalender, kanban og årshjul bruger samme
referencer. Opgavearbejde ændrer ingen ledger, betaling eller indberetning.
Første komplette levering omfatter hele den aftalte oplevelse. Påmindelser
er brugeraktiverede og leveres inde i Rentemester; ingen mail eller LLM-runner.

Produktkode og tests bruger syntetiske data. Private virksomhedsdokumenter
kopieres ikke ind i repository, issues eller PR'er. Ekstern kalenderintegration,
ressourceplanlægning og automatisk juridisk vedligeholdelse er uden for opgaven.

## Implementeringsgrænse

Workspace-migration og core-opgaver/boards; kildeadaptere, serier og påmindelser;
HTTP, CLI og MCP; fire cockpit-visninger; snapshot/restore, adgang og runtime.
Eksisterende kildesider og deres kontrakter bevares.

## Aflevering

Acceptmatrix A01–A36 med konkrete testreferencer, samlet syntetisk browserforløb,
adversarial review og fuld lokal verifikation. Den repræsentative brugertest
med ét-minutsmålet kræver en faktisk brugerobservation og må ikke erstattes
af en påstand baseret på screenshots eller automatiske tests.

Kode og dokumenter opdateres i Graphify før commits og aflevering. En kandidat
bygges gennem eksisterende releaseflow; produktionspromotion følger de gældende
godkendelser.
