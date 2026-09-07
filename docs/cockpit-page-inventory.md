# Cockpit: inventar over sideskabeloner

Dette er inventaret for #649. Det grupperer synlige firmaruter efter fælles
sideskabelon, ikke efter URL. Kilden er
`app/src/company-page-inventory.ts`; testen `company-page-inventory.test.ts`
sammenligner den med route-registret og fejler, hvis en ny rute mangler.

| Sideskabelon | Status | Ruter |
| --- | --- | --- |
| Overblik og arbejdsstatus | Kræver mutation | dashboard, suggestions, exceptions, workspace-inbox |
| Bankregister og afstemning | Undersøgt | bank |
| Filtreret register | Kræver mutation | journal, drafts, documents, payables, purchase-overview, mileage, assets, invoices, invoice-templates, contacts, accounts, dimensions, bank-accounts, receipt-email |
| Bogføringsarbejdsgang | Kræver mutation | approval-policy, posting-rules, batch-bookkeeping, period-lock, accruals |
| Finansiel rapport | Utilgængelig | income-statement, balance, trial-balance, obligations, liquidity, budget, multi-year, annual-report, vat |
| Virksomhedsadministration | Kræver mutation | workspace-register, archive, manage, retention, integrity, gdpr |

"Kræver mutation" betyder alene, at den centrale opgave ikke kan undersøges
færdigt uden en reversibel, syntetisk testhandling. "Utilgængelig" betyder, at
der mangler et fælles syntetisk korpus — ikke at den eksisterende URL er fjernet
eller blokeret.
