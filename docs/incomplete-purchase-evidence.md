# Ufuldstændige standardkøbsbilag

Et ufuldstændigt købsbilag er stadig den oprindelige udsteders dokument. Det
kan indlæses, men Rentemester finder aldrig på et køber-CVR, en adresse eller
en ny modtager. Markøren `incompleteStandardPurchaseInvoice` siger alene, at
et ellers standardiseret købsbilag mangler faktiske køberfelter.

Eksempel på metadata til `documents ingest` eller `documents_ingest`:

```json
{
  "source": "selected-upload",
  "documentType": "purchase_sale",
  "incompleteStandardPurchaseInvoice": true,
  "issueDate": "2026-08-15",
  "invoiceNo": "SYN-1007",
  "deliveryDescription": "Synthetic professional service",
  "amountIncVat": 1250,
  "vatAmount": 250,
  "currency": "DKK",
  "sender": { "name": "Synthetic Supplier", "address": "Example Street 1", "vatOrCvr": "DK12345678", "countryCode": "DK", "identifierKind": "dk_cvr" },
  "recipient": { "name": "Example Company ApS" }
}
```

Efter indlæsning kan en person med relevant adgang tilføje `sourceReference`
og `businessUseReason` via `documents set-company-context`,
`documents_set_company_context` eller Bilag-Cockpit. Det er en append-only,
actor-auditeret attribution bundet til både original- og metadatahash. Det er
ikke en rettelse af fakturaen og heller ikke en moms-godkendelse.

Kør derefter den normale read-only `expense vat-preflight`. Den anvender den
samme dokumentationskontrol som bogføring. Hvis den afviser fakturaen, må
agenten ikke falde tilbage til en intern voucher eller SQL; den kan kun vælge
en understøttet ikke-momspligtig behandling eller indhente yderligere,
dokumenteret bevis. Et fraværende køber-CVR og en fraværende køberadresse er
bevidst separate forhold; et bilag bliver ikke forenklet alene fordi et felt
mangler.
