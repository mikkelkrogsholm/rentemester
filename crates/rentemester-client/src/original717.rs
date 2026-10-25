//! Work-Issue: #717. Frozen original contracts over the single admitted server.
use crate::{ApiClient, Error, company_path};
use base64::Engine;
use serde_json::{Value, json};
const CATALOGUE: &str = r###"[{"name":"system_backup","title":"Create system backup","description":"Opretter revisionsklar backup. Med archive:true pakkes backuppen straks til ét .tar-arkiv klar til off-site placering. write-irreversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"at":{"description":"Optional ISO-8601 timestamp the backup is created at (overrides the wall clock for deterministic testing). Default: current UTC time.","type":"string"},"archive":{"description":"When true, ALSO pack the backup into a single deterministic .tar (plus a .sha256 sidecar) ready for off-site placement. Default: false — the on-disk backup directory is created without an archive.","type":"boolean"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"system_backup_archive","title":"Pack a backup into a single-file archive","description":"Pakker en eksisterende backup til ét deterministisk .tar-arkiv (+ .sha256) klar til at agenten kan flytte det off-site. Uden backupId pakkes den nyeste. write-irreversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"backupId":{"description":"Backup id to pack (returned by `system_backup` or visible in `system_backup_status`). When omitted, the most recent backup is selected.","type":"string"},"out":{"description":"Optional output path for the .tar archive (default: alongside the backup directory). A .sha256 sidecar is always written next to it.","type":"string"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"system_backup_place","title":"Place a backup archive at a destination","description":"Kopierer et backup-arkiv til en destination med en lokal/synkroniseret mappe (fx en Dropbox-desktopmappe) og verificerer kopien med sha256. write-irreversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"archivePath":{"type":"string","minLength":1,"description":"Absolute path to the .tar archive produced by `system_backup` with archive:true (or `system_backup_archive`). The .sha256 sidecar is located automatically next to it."},"destinationId":{"type":"string","minLength":1,"description":"Destination id (from `system_backup_destination_list`) to copy the archive to. The destination's `location` must be a local or synced folder reachable by this process."},"actorKind":{"description":"Who is performing the placement: 'human' for a person clicking a button, 'agent' for an AI/automation. Default: 'agent'.","type":"string","enum":["human","agent"]},"at":{"description":"Optional ISO-8601 timestamp recorded in the placement log (default: current UTC time).","type":"string"},"note":{"description":"Optional free-text note attached to the placement record.","type":"string"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","archivePath","destinationId"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"system_backup_verify_remote_placement","title":"Verify remote backup placement evidence","description":"Verifies remote backup metadata and downloaded content through an injected provider adapter. Object id, name, parent, size and checksum must match; declared evidence is never upgraded. write-irreversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":true},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"destinationId":{"type":"string","minLength":1,"description":"Destination id from system_backup_destination_list."},"backupId":{"type":"string","minLength":1,"description":"Backup id returned by system_backup."},"remoteObjectId":{"type":"string","minLength":1,"description":"Provider object id of the placed archive."},"maxMetadataAgeMs":{"description":"Maximum acceptable provider metadata age; defaults to five minutes.","type":"integer","minimum":0,"maximum":9007199254740991},"actorKind":{"description":"Who performed the placement; default agent.","type":"string","enum":["human","agent"]},"at":{"description":"When the backup was placed; defaults to current UTC time and cannot be future-dated.","type":"string"},"note":{"description":"Optional placement note.","type":"string"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","destinationId","backupId","remoteObjectId"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"system_restore_backup","title":"Restore system backup (DESTRUCTIVE)","description":"Gendanner backup til ny virksomhedssti. DESTRUCTIVE — kræver confirm:true OG confirmText='RESTORE <targetCompany>'. Sletter ikke nogen filer på source, men kan overskrive filer i targetCompany.","annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"backupDir":{"type":"string","minLength":1,"description":"Path to the backup directory (or .tar archive) to restore from. ON THE MCP SERVER'S FILESYSTEM. Never modified by the restore."},"targetCompany":{"type":"string","minLength":1,"description":"Path to the company directory the backup is restored INTO. Existing files here may be overwritten."},"verifyKey":{"description":"Optional path to the SYMMETRIC HMAC verification key (the '.backup-manifest.key' file) used to verify the backup manifest's HMAC tag. This is NOT the ed25519 public key — see publicKey for that. Typically required when backupDir is a .tar archive; for a backup directory still inside its company 'backups/' folder the key is otherwise inferred. Mirrors the CLI's --verify-key.","type":"string"},"publicKey":{"description":"Optional path to the ASYMMETRIC ed25519 public key used to verify the backup manifest's ed25519 signature (the signature added by 'system backup --sign-with-ed25519'). Supplying it out-of-band lets an independent third party verify authenticity without the HMAC key. Distinct from verifyKey, which is the symmetric HMAC key. Mirrors the CLI's --public-key.","type":"string"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"},"confirmText":{"description":"Destructive-confirmation text. Must be EXACTLY 'RESTORE <targetCompany>' — the literal word RESTORE, a space, then your targetCompany argument verbatim. Concrete example: for targetCompany='/data/acme', use confirmText='RESTORE /data/acme'. Deliberately schema-OPTIONAL (#307): a missing/empty value reaches the handler and yields the normal { ok:false, errors:[...] } envelope (with code:'CONFIRMTEXT_MISMATCH'), exactly like a mismatch — never a raw -32602. Any mismatch (or omission) rejects the call.","type":"string"}},"required":["backupDir","targetCompany"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"mail_intake_ingest","title":"Ingest bilagsmail","description":"Indlæser en lokal .eml-fil eller maildrop-mappe, parser vedhæftninger (PDF/JPG/PNG) og videresender dem til document-ingest-pipelinen. Deduplikerer på message-id + attachment-hash så reruns er idempotente. Beskeder uden brugbar vedhæftning eller med tvetydig metadata routes til exception-køen. Kræver confirm:true. write-reversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"source":{"type":"string","minLength":1,"description":"Sti til en .eml-fil eller en maildrop-mappe"},"metadata":{"type":"object","properties":{"documentType":{"description":"Document type (default 'purchase_sale').","type":"string","enum":["purchase_sale","cash_register_receipt","internal_voucher"]},"issueDate":{"description":"Document/invoice date in YYYY-MM-DD format.","type":"string"},"invoiceNo":{"description":"Invoice or receipt number printed on the document.","type":"string"},"deliveryDescription":{"description":"Free-text description of the goods or services.","type":"string"},"amountIncVat":{"description":"Total amount including VAT, in kroner (decimal DKK, 2 decimals — NOT øre).","type":"number"},"currency":{"description":"3-letter ISO currency code (default 'DKK').","type":"string"},"sender":{"description":"Sender/supplier details.","type":"object","properties":{"name":{"description":"Party name.","type":"string"},"address":{"description":"Party postal address.","type":"string"},"vatOrCvr":{"description":"Party VAT or CVR number, e.g. 'DK12345678'.","type":"string"},"countryCode":{"description":"Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind.","type":"string","minLength":2,"maxLength":2},"identifierKind":{"description":"Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU.","type":"string","enum":["dk_cvr","eu_vat","non_eu"]}},"additionalProperties":false},"recipient":{"description":"Recipient/buyer details.","type":"object","properties":{"name":{"description":"Party name.","type":"string"},"address":{"description":"Party postal address.","type":"string"},"vatOrCvr":{"description":"Party VAT or CVR number, e.g. 'DK12345678'.","type":"string"},"countryCode":{"description":"Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind.","type":"string","minLength":2,"maxLength":2},"identifierKind":{"description":"Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU.","type":"string","enum":["dk_cvr","eu_vat","non_eu"]}},"additionalProperties":false},"vatAmount":{"description":"VAT amount, in kroner (decimal DKK, 2 decimals — NOT øre).","type":"number"},"purchaseVatLines":{"description":"Optional durable purchase VAT split. Its net and VAT totals must reconcile exactly with the document totals.","minItems":1,"type":"array","items":{"type":"object","properties":{"classification":{"type":"string","enum":["dk_purchase_25","exempt"]},"netAmount":{"type":"number","minimum":0,"description":"Tax base in kroner."},"vatAmount":{"description":"VAT amount in kroner; 25% for dk_purchase_25, otherwise zero.","type":"number","minimum":0}},"required":["classification","netAmount"],"additionalProperties":false}},"reverseChargeWordingConfirmed":{"description":"True only when a human has confirmed that the supplier invoice contains reverse-charge wording; required with the other invoice evidence before non-EU input-VAT deduction.","type":"boolean"},"danishSimplifiedPurchaseInvoice":{"description":"Explicit source fact: this is a Danish simplified purchase invoice. This never changes recipient invoice fields.","type":"boolean"},"paymentDetails":{"description":"Free-text payment details, e.g. 'Bankoverførsel 2026-05-17'.","type":"string"},"exemptionCode":{"description":"Set to 'FOREIGN_PHYSICAL_ONLY' for a foreign physical-only receipt; otherwise omit.","anyOf":[{"type":"string","const":"FOREIGN_PHYSICAL_ONLY"},{"type":"null"}]},"sourceBankTransactionId":{"description":"Required for internal_voucher: the imported bank transaction that is its primary evidence.","type":"integer","exclusiveMinimum":0,"maximum":9007199254740991},"accountingRationale":{"description":"Required for internal_voucher: the accounting reason for the posting.","type":"string","minLength":1,"maxLength":2000}},"additionalProperties":{},"description":"DocumentMetadata-payload (uden 'source') der anvendes på vedhæftningerne — se examples/bilagsmail.metadata.json"},"metadataPerMessage":{"description":"Metadata pr. message-id; overstyrer 'metadata' for den pågældende besked","type":"object","propertyNames":{"type":"string"},"additionalProperties":{"type":"object","properties":{"documentType":{"description":"Document type (default 'purchase_sale').","type":"string","enum":["purchase_sale","cash_register_receipt","internal_voucher"]},"issueDate":{"description":"Document/invoice date in YYYY-MM-DD format.","type":"string"},"invoiceNo":{"description":"Invoice or receipt number printed on the document.","type":"string"},"deliveryDescription":{"description":"Free-text description of the goods or services.","type":"string"},"amountIncVat":{"description":"Total amount including VAT, in kroner (decimal DKK, 2 decimals — NOT øre).","type":"number"},"currency":{"description":"3-letter ISO currency code (default 'DKK').","type":"string"},"sender":{"description":"Sender/supplier details.","type":"object","properties":{"name":{"description":"Party name.","type":"string"},"address":{"description":"Party postal address.","type":"string"},"vatOrCvr":{"description":"Party VAT or CVR number, e.g. 'DK12345678'.","type":"string"},"countryCode":{"description":"Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind.","type":"string","minLength":2,"maxLength":2},"identifierKind":{"description":"Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU.","type":"string","enum":["dk_cvr","eu_vat","non_eu"]}},"additionalProperties":false},"recipient":{"description":"Recipient/buyer details.","type":"object","properties":{"name":{"description":"Party name.","type":"string"},"address":{"description":"Party postal address.","type":"string"},"vatOrCvr":{"description":"Party VAT or CVR number, e.g. 'DK12345678'.","type":"string"},"countryCode":{"description":"Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind.","type":"string","minLength":2,"maxLength":2},"identifierKind":{"description":"Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU.","type":"string","enum":["dk_cvr","eu_vat","non_eu"]}},"additionalProperties":false},"vatAmount":{"description":"VAT amount, in kroner (decimal DKK, 2 decimals — NOT øre).","type":"number"},"purchaseVatLines":{"description":"Optional durable purchase VAT split. Its net and VAT totals must reconcile exactly with the document totals.","minItems":1,"type":"array","items":{"type":"object","properties":{"classification":{"type":"string","enum":["dk_purchase_25","exempt"]},"netAmount":{"type":"number","minimum":0,"description":"Tax base in kroner."},"vatAmount":{"description":"VAT amount in kroner; 25% for dk_purchase_25, otherwise zero.","type":"number","minimum":0}},"required":["classification","netAmount"],"additionalProperties":false}},"reverseChargeWordingConfirmed":{"description":"True only when a human has confirmed that the supplier invoice contains reverse-charge wording; required with the other invoice evidence before non-EU input-VAT deduction.","type":"boolean"},"danishSimplifiedPurchaseInvoice":{"description":"Explicit source fact: this is a Danish simplified purchase invoice. This never changes recipient invoice fields.","type":"boolean"},"paymentDetails":{"description":"Free-text payment details, e.g. 'Bankoverførsel 2026-05-17'.","type":"string"},"exemptionCode":{"description":"Set to 'FOREIGN_PHYSICAL_ONLY' for a foreign physical-only receipt; otherwise omit.","anyOf":[{"type":"string","const":"FOREIGN_PHYSICAL_ONLY"},{"type":"null"}]},"sourceBankTransactionId":{"description":"Required for internal_voucher: the imported bank transaction that is its primary evidence.","type":"integer","exclusiveMinimum":0,"maximum":9007199254740991},"accountingRationale":{"description":"Required for internal_voucher: the accounting reason for the posting.","type":"string","minLength":1,"maxLength":2000}},"additionalProperties":{},"description":"DocumentMetadata-payload (uden 'source') der anvendes på vedhæftningerne — se examples/bilagsmail.metadata.json"}},"force":{"type":"boolean"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","source"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"imap_intake_poll","title":"Poll bilagsmail via IMAP","description":"Poller en hosted IMAP-postkasse, henter nye beskeder og videresender vedhæftninger (PDF/JPG/PNG) til den eksisterende bilagsmail-pipeline (#122). Dedup på message-id + attachment-hash er rerun-stabil — gentaget poll skaber ingen dubletter. Beskeder uden brugbar vedhæftning routes til exception-køen. IMAP-credentials kommer fra argumenter og RENTEMESTER_IMAP_PASSWORD i miljøet, aldrig fra ledger'en. Kræver confirm:true. write-reversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":true},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"imapHost":{"description":"IMAP-host; standard RENTEMESTER_IMAP_HOST","type":"string","minLength":1},"imapPort":{"description":"IMAP-port; standard 993 (TLS)","type":"integer","exclusiveMinimum":0,"maximum":9007199254740991},"imapUsername":{"description":"IMAP-brugernavn; standard RENTEMESTER_IMAP_USERNAME","type":"string","minLength":1},"imapMailbox":{"description":"Mailbox; standard INBOX","type":"string","minLength":1},"sinceUid":{"description":"Valgfri UID-nedre grænse","type":"integer","minimum":0,"maximum":9007199254740991},"metadata":{"type":"object","properties":{"documentType":{"description":"Document type (default 'purchase_sale').","type":"string","enum":["purchase_sale","cash_register_receipt","internal_voucher"]},"issueDate":{"description":"Document/invoice date in YYYY-MM-DD format.","type":"string"},"invoiceNo":{"description":"Invoice or receipt number printed on the document.","type":"string"},"deliveryDescription":{"description":"Free-text description of the goods or services.","type":"string"},"amountIncVat":{"description":"Total amount including VAT, in kroner (decimal DKK, 2 decimals — NOT øre).","type":"number"},"currency":{"description":"3-letter ISO currency code (default 'DKK').","type":"string"},"sender":{"description":"Sender/supplier details.","type":"object","properties":{"name":{"description":"Party name.","type":"string"},"address":{"description":"Party postal address.","type":"string"},"vatOrCvr":{"description":"Party VAT or CVR number, e.g. 'DK12345678'.","type":"string"},"countryCode":{"description":"Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind.","type":"string","minLength":2,"maxLength":2},"identifierKind":{"description":"Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU.","type":"string","enum":["dk_cvr","eu_vat","non_eu"]}},"additionalProperties":false},"recipient":{"description":"Recipient/buyer details.","type":"object","properties":{"name":{"description":"Party name.","type":"string"},"address":{"description":"Party postal address.","type":"string"},"vatOrCvr":{"description":"Party VAT or CVR number, e.g. 'DK12345678'.","type":"string"},"countryCode":{"description":"Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind.","type":"string","minLength":2,"maxLength":2},"identifierKind":{"description":"Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU.","type":"string","enum":["dk_cvr","eu_vat","non_eu"]}},"additionalProperties":false},"vatAmount":{"description":"VAT amount, in kroner (decimal DKK, 2 decimals — NOT øre).","type":"number"},"purchaseVatLines":{"description":"Optional durable purchase VAT split. Its net and VAT totals must reconcile exactly with the document totals.","minItems":1,"type":"array","items":{"type":"object","properties":{"classification":{"type":"string","enum":["dk_purchase_25","exempt"]},"netAmount":{"type":"number","minimum":0,"description":"Tax base in kroner."},"vatAmount":{"description":"VAT amount in kroner; 25% for dk_purchase_25, otherwise zero.","type":"number","minimum":0}},"required":["classification","netAmount"],"additionalProperties":false}},"reverseChargeWordingConfirmed":{"description":"True only when a human has confirmed that the supplier invoice contains reverse-charge wording; required with the other invoice evidence before non-EU input-VAT deduction.","type":"boolean"},"danishSimplifiedPurchaseInvoice":{"description":"Explicit source fact: this is a Danish simplified purchase invoice. This never changes recipient invoice fields.","type":"boolean"},"paymentDetails":{"description":"Free-text payment details, e.g. 'Bankoverførsel 2026-05-17'.","type":"string"},"exemptionCode":{"description":"Set to 'FOREIGN_PHYSICAL_ONLY' for a foreign physical-only receipt; otherwise omit.","anyOf":[{"type":"string","const":"FOREIGN_PHYSICAL_ONLY"},{"type":"null"}]},"sourceBankTransactionId":{"description":"Required for internal_voucher: the imported bank transaction that is its primary evidence.","type":"integer","exclusiveMinimum":0,"maximum":9007199254740991},"accountingRationale":{"description":"Required for internal_voucher: the accounting reason for the posting.","type":"string","minLength":1,"maxLength":2000}},"additionalProperties":{},"description":"DocumentMetadata-payload (uden 'source') der anvendes på vedhæftningerne — se examples/bilagsmail.metadata.json"},"metadataPerMessage":{"description":"Metadata pr. message-id; overstyrer 'metadata' for den pågældende besked","type":"object","propertyNames":{"type":"string"},"additionalProperties":{"type":"object","properties":{"documentType":{"description":"Document type (default 'purchase_sale').","type":"string","enum":["purchase_sale","cash_register_receipt","internal_voucher"]},"issueDate":{"description":"Document/invoice date in YYYY-MM-DD format.","type":"string"},"invoiceNo":{"description":"Invoice or receipt number printed on the document.","type":"string"},"deliveryDescription":{"description":"Free-text description of the goods or services.","type":"string"},"amountIncVat":{"description":"Total amount including VAT, in kroner (decimal DKK, 2 decimals — NOT øre).","type":"number"},"currency":{"description":"3-letter ISO currency code (default 'DKK').","type":"string"},"sender":{"description":"Sender/supplier details.","type":"object","properties":{"name":{"description":"Party name.","type":"string"},"address":{"description":"Party postal address.","type":"string"},"vatOrCvr":{"description":"Party VAT or CVR number, e.g. 'DK12345678'.","type":"string"},"countryCode":{"description":"Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind.","type":"string","minLength":2,"maxLength":2},"identifierKind":{"description":"Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU.","type":"string","enum":["dk_cvr","eu_vat","non_eu"]}},"additionalProperties":false},"recipient":{"description":"Recipient/buyer details.","type":"object","properties":{"name":{"description":"Party name.","type":"string"},"address":{"description":"Party postal address.","type":"string"},"vatOrCvr":{"description":"Party VAT or CVR number, e.g. 'DK12345678'.","type":"string"},"countryCode":{"description":"Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind.","type":"string","minLength":2,"maxLength":2},"identifierKind":{"description":"Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU.","type":"string","enum":["dk_cvr","eu_vat","non_eu"]}},"additionalProperties":false},"vatAmount":{"description":"VAT amount, in kroner (decimal DKK, 2 decimals — NOT øre).","type":"number"},"purchaseVatLines":{"description":"Optional durable purchase VAT split. Its net and VAT totals must reconcile exactly with the document totals.","minItems":1,"type":"array","items":{"type":"object","properties":{"classification":{"type":"string","enum":["dk_purchase_25","exempt"]},"netAmount":{"type":"number","minimum":0,"description":"Tax base in kroner."},"vatAmount":{"description":"VAT amount in kroner; 25% for dk_purchase_25, otherwise zero.","type":"number","minimum":0}},"required":["classification","netAmount"],"additionalProperties":false}},"reverseChargeWordingConfirmed":{"description":"True only when a human has confirmed that the supplier invoice contains reverse-charge wording; required with the other invoice evidence before non-EU input-VAT deduction.","type":"boolean"},"danishSimplifiedPurchaseInvoice":{"description":"Explicit source fact: this is a Danish simplified purchase invoice. This never changes recipient invoice fields.","type":"boolean"},"paymentDetails":{"description":"Free-text payment details, e.g. 'Bankoverførsel 2026-05-17'.","type":"string"},"exemptionCode":{"description":"Set to 'FOREIGN_PHYSICAL_ONLY' for a foreign physical-only receipt; otherwise omit.","anyOf":[{"type":"string","const":"FOREIGN_PHYSICAL_ONLY"},{"type":"null"}]},"sourceBankTransactionId":{"description":"Required for internal_voucher: the imported bank transaction that is its primary evidence.","type":"integer","exclusiveMinimum":0,"maximum":9007199254740991},"accountingRationale":{"description":"Required for internal_voucher: the accounting reason for the posting.","type":"string","minLength":1,"maxLength":2000}},"additionalProperties":{},"description":"DocumentMetadata-payload (uden 'source') der anvendes på vedhæftningerne — se examples/bilagsmail.metadata.json"}},"force":{"type":"boolean"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"invoice_send_email","title":"Send invoice or reminder by email","description":"Sender en udstedt faktura eller en betalingspaamindelse til kundens email via SMTP med PDF'en vedhaeftet og registrerer afsendelsen append-only. SMTP har ingen provider-reconciliation-kontrakt: læs den append-only delivery-evidens før et retry. SMTP-config laeses fra filen config/smtp.json i virksomhedsmappen; credentials gemmes aldrig i bogføringstilstanden. config/smtp.json er et JSON-objekt med PAAKRAEVEDE felter host (string), port (number) og fromAddress (string), samt VALGFRIE felter fromName, username, password og dryRun (boolean). Mangler filen, afvises kaldet med envelopen { ok:false, errors:[\"missing SMTP config: create config/smtp.json ...\"] } — der kastes ingen exception. Den indbyggede SMTP-transport koerer KUN i dry-run: med dryRun:true registreres afsendelsen uden et netvaerkskald (ok:true); uden dryRun:true fejler et rigtigt send med envelopen { ok:false, errors:[\"the built-in SMTP transport runs in dryRun only ...\"] }. Aegte levering kraever en injiceret EmailTransport. write-irreversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":true},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"documentId":{"description":"Document ID of the issued invoice to send. Provide this OR invoiceNumber.","type":"integer","exclusiveMinimum":0,"maximum":9007199254740991},"invoiceNumber":{"description":"Invoice number of the issued invoice to send. Provide this OR documentId.","type":"string"},"kind":{"description":"What to send (default 'invoice'): 'invoice' = the invoice itself; 'reminder' = a payment reminder.","type":"string","enum":["invoice","reminder"]},"to":{"description":"Optional recipient email address. When omitted, the customer's stored email is used.","type":"string"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"efaktura_konfigurer","title":"Gem Digisense API license-key","description":"Gemmer Digisense API license-key i secret-laget (config/digisense.json, 0600). Dette er PRECONDITION'en for efaktura_registrer/efaktura_modtag/efaktura_send — uden en gemt key fejler de tre med 'Digisense er ikke konfigureret'. license-key er et SECRET og rammer ALDRIG ledger'en. Kræver confirm:true. write-irreversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"apiLicenseKey":{"type":"string","minLength":1,"description":"Digisense API license-key (én nøgle for hele licensen). Gemmes kun i config/digisense.json, aldrig i ledger'en."},"environment":{"description":"prod/test base-URL switch; standard 'test'.","type":"string","enum":["production","test"]},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","apiLicenseKey"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}}]"###;
pub fn original717_tools() -> Vec<Value> {
    serde_json::from_str(CATALOGUE).expect("static source catalogue")
}
// Validate the frozen source grammar, including passthrough metadata and records.
fn valid(v: &Value, s: &Value) -> bool {
    if let Some(a) = s["anyOf"].as_array() {
        return a.iter().any(|s| valid(v, s));
    }
    if s.get("const").is_some_and(|c| c != v)
        || s["enum"].as_array().is_some_and(|a| !a.contains(v))
    {
        return false;
    }
    match s["type"].as_str() {
        Some("object") => v.as_object().is_some_and(|o| {
            s["required"].as_array().is_none_or(|r| {
                r.iter()
                    .all(|k| k.as_str().is_some_and(|k| o.contains_key(k)))
            }) && o.iter().all(|(k, v)| match s["properties"].get(k) {
                Some(s) => valid(v, s),
                None => {
                    s["additionalProperties"] != false
                        && (!s["additionalProperties"].is_object()
                            || valid(v, &s["additionalProperties"]))
                }
            })
        }),
        Some("array") => v.as_array().is_some_and(|a| {
            s["minItems"].as_u64().is_none_or(|m| a.len() as u64 >= m)
                && s["maxItems"].as_u64().is_none_or(|m| a.len() as u64 <= m)
                && a.iter().all(|v| valid(v, &s["items"]))
        }),
        Some("string") => v.as_str().is_some_and(|t| {
            let n = t.encode_utf16().count() as u64;
            s["minLength"].as_u64().is_none_or(|m| n >= m)
                && s["maxLength"].as_u64().is_none_or(|m| n <= m)
        }),
        Some("integer") => v.as_i64().is_some_and(|n| {
            s["minimum"].as_i64().is_none_or(|m| n >= m)
                && s["exclusiveMinimum"].as_i64().is_none_or(|m| n > m)
                && s["maximum"].as_i64().is_none_or(|m| n <= m)
        }),
        Some("number") => v
            .as_f64()
            .is_some_and(|n| s["minimum"].as_f64().is_none_or(|m| n >= m)),
        Some("boolean") => v.is_boolean(),
        Some("null") => v.is_null(),
        _ => true,
    }
}
pub fn validate_original717(name: &str, args: &Value) -> Result<(), Error> {
    let tool = original717_tools()
        .into_iter()
        .find(|t| t["name"] == name)
        .ok_or(Error::InvalidInput("unknown original717 operation"))?;
    if !valid(args, &tool["inputSchema"]) {
        return Err(Error::InvalidInput("invalid original717 arguments"));
    }
    Ok(())
}
fn core(name: &str, value: Value) -> Result<Value, Error> {
    let r = &value["result"];
    if value["ok"] != true
        || !r.is_object()
        || !r["ok"].is_boolean()
        || r.get("errors")
            .is_some_and(|v| !v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
    {
        return Err(Error::Protocol);
    }
    if r["ok"] == false {
        if !r["errors"].as_array().is_some_and(|a| !a.is_empty()) {
            return Err(Error::Protocol);
        }
    } else {
        let fields: &[&str] = match name {
            "system_backup" => &["backupId", "backupDir", "manifestPath", "dbSnapshotPath"],
            "system_backup_archive" => &["backupId", "archivePath", "archiveSha256", "sha256Path"],
            "system_backup_place" | "system_backup_verify_remote_placement" => &[],
            "system_restore_backup" => &[
                "backupId",
                "restoredAt",
                "targetCompanyRoot",
                "restoredDbPath",
            ],
            "efaktura_konfigurer" => &["configPath", "environment"],
            "invoice_send_email" => &["invoiceNumber", "kind", "recipient", "subject", "messageId"],
            _ => &[],
        };
        if fields.iter().any(|f| !r[*f].is_string()) {
            return Err(Error::Protocol);
        }
        if matches!(
            name,
            "system_backup_place" | "system_backup_verify_remote_placement"
        ) && !r["placement"].is_object()
        {
            return Err(Error::Protocol);
        }
        if name == "system_backup_archive" && !r["archiveSizeBytes"].is_u64() {
            return Err(Error::Protocol);
        }
        if name == "invoice_send_email" && !r["duplicate"].is_boolean() {
            return Err(Error::Protocol);
        }
        if matches!(name, "mail_intake_ingest" | "imap_intake_poll")
            && (!r["documents"].is_array()
                || [
                    "messagesProcessed",
                    "attachmentsIngested",
                    "attachmentsSkipped",
                    "exceptionsCreated",
                ]
                .iter()
                .any(|f| !r[*f].is_u64())
                || name == "imap_intake_poll" && !r["messagesFetched"].is_u64())
        {
            return Err(Error::Protocol);
        }
    }
    if name == "efaktura_konfigurer" && r["ok"] == true {
        // Secret-writing operation returns only its source-supported public receipt.
        return Ok(json!({"ok":true,"configPath":r["configPath"],"environment":r["environment"]}));
    }
    Ok(r.clone())
}
// MCP-local files are bytes, never server filesystem authority. No symlink/special
// files; bounded opens and reads prevent a replaced file from becoming unbounded.
fn messages(source: &str) -> Result<Value, Error> {
    use std::{fs, io::Read, path::Path};
    let fail = || {
        Error::InvalidInput(
            "mail source must be bounded regular local EML files (max32, 25MiB total); no symlinks",
        )
    };
    let p = Path::new(source);
    let m = fs::symlink_metadata(p).map_err(|_| fail())?;
    if m.file_type().is_symlink() {
        return Err(fail());
    }
    let mut paths = if m.is_dir() {
        let entries = fs::read_dir(p)
            .map_err(|_| fail())?
            .take(4097)
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| fail())?;
        // Count before filtering: a bounded traversal must not silently omit a
        // later EML after thousands of unrelated entries.
        if entries.len() > 4096 {
            return Err(fail());
        }
        entries
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("eml")))
            .collect::<Vec<_>>()
    } else {
        vec![p.to_owned()]
    };
    if paths.len() > 4096 {
        return Err(fail());
    }
    paths.sort();
    if paths.len() > 32 {
        return Err(fail());
    }
    let mut result = Vec::new();
    let mut total = 0;
    for p in paths {
        let m = fs::symlink_metadata(&p).map_err(|_| fail())?;
        if !m.is_file() || m.file_type().is_symlink() {
            return Err(fail());
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(0x100 | 0x4);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(0x20000 | 0x800);
        } // macOS O_NOFOLLOW | O_NONBLOCK
        let f = options.open(&p).map_err(|_| fail())?;
        if !f.metadata().map_err(|_| fail())?.is_file() {
            return Err(fail());
        }
        let mut bytes = Vec::new();
        f.take(25 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail())?;
        total += bytes.len();
        if total > 25 * 1024 * 1024 {
            return Err(fail());
        }
        result.push(json!({"source":p.to_str().ok_or_else(fail)?,"base64":base64::engine::general_purpose::STANDARD.encode(bytes)}));
    }
    Ok(json!(result))
}
impl ApiClient {
    pub async fn original717(&self, name: &str, args: &Value) -> Result<Value, Error> {
        validate_original717(name, args)?;
        if args["confirm"] != true {
            return Err(Error::InvalidInput("explicit confirmation required"));
        }
        let restore = name == "system_restore_backup";
        // Source authenticated remote restore had no company and was unsupported.
        // Explicit out is an operator-local write, not arbitrary hosted authority.
        let explicit_out = args["out"].as_str().is_some_and(|s| !s.trim().is_empty());
        if (restore || explicit_out) && self.local.is_none() {
            return Err(Error::InvalidInput(
                "explicit verified local operator profile required",
            ));
        }
        let mut body = args.clone();
        if restore {
            // Confirm the caller's literal target before resolving it. Rewriting
            // an incorrect confirmation would grant an unconfirmed restore.
            let target = args["targetCompany"].as_str().ok_or(Error::Protocol)?;
            if args["confirmText"].as_str() != Some(format!("RESTORE {target}").as_str()) {
                return Err(Error::InvalidInput(
                    "restore confirmation must match target",
                ));
            }
            for field in ["backupDir", "targetCompany", "verifyKey", "publicKey"] {
                if let Some(value) = args[field].as_str().filter(|s| !s.is_empty()) {
                    body[field] = json!(local_path(value)?);
                }
            }
            body["confirmText"] = json!(format!(
                "RESTORE {}",
                body["targetCompany"].as_str().ok_or(Error::Protocol)?
            ));
        } else if explicit_out {
            body["out"] = json!(local_path(
                args["out"].as_str().ok_or(Error::Protocol)?.trim()
            )?);
        }
        let path = if restore {
            "/api/original-system-restore-backup".to_owned()
        } else {
            let company = self.diagnostics_company_original118(
                args["company"]
                    .as_str()
                    .ok_or(Error::InvalidInput("company required"))?,
            )?;
            body.as_object_mut()
                .ok_or(Error::Protocol)?
                .remove("company");
            let route = match name {
                "system_backup" => "original-system-backup",
                "system_backup_archive" => "original-system-backup-archive",
                "system_backup_place" => "original-system-backup-place",
                "system_backup_verify_remote_placement" => {
                    "original-system-backup-verify-remote-placement"
                }
                "mail_intake_ingest" => "original-mail-intake",
                "imap_intake_poll" => "original-imap-intake",
                "invoice_send_email" => "original-invoice-send-email",
                "efaktura_konfigurer" => "original-efaktura-konfigurer",
                _ => return Err(Error::InvalidInput("unknown operation")),
            };
            company_path(&company, route)?
        };
        if name == "mail_intake_ingest" {
            let files = messages(
                args["source"]
                    .as_str()
                    .ok_or(Error::InvalidInput("source required"))?,
            )?;
            body.as_object_mut()
                .ok_or(Error::Protocol)?
                .remove("source");
            body["messages"] = files;
        }
        let bytes = serde_json::to_vec(&body).map_err(|_| Error::InvalidInput("invalid body"))?;
        if bytes.len()
            > if name == "mail_intake_ingest" {
                90 * 1024 * 1024
            } else {
                16384
            }
        {
            return Err(Error::InvalidInput("original body exceeds transport limit"));
        }
        core(
            name,
            self.request(reqwest::Method::POST, &path, &[], Some(bytes))
                .await?,
        )
    }
    pub fn original717_company_selector(&self, company: &str) -> Result<String, Error> {
        self.diagnostics_company_original118(company)
    }
}

// Resolve source-local relative paths lexically. The admitted server still
// verifies actual ancestors, ownership, leases and protected target boundaries.
fn local_path(value: &str) -> Result<String, Error> {
    use std::path::{Component, Path, PathBuf};
    let path = Path::new(value);
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| Error::InvalidInput("local working directory unavailable"))?
            .join(path)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            other => resolved.push(other.as_os_str()),
        }
    }
    resolved
        .into_os_string()
        .into_string()
        .map_err(|_| Error::InvalidInput("local path must be UTF-8"))
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn original717_maildrop_refuses_truncated_directory() {
        let dir = std::env::temp_dir().join(format!(
            "rm717-mail-bounds-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        for n in 0..4097 {
            std::fs::write(dir.join(format!("{n:05}.txt")), []).unwrap();
        }
        let result = messages(dir.to_str().unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(result, Err(Error::InvalidInput(_))));
    }
}

impl ApiClient {
    /// CLI boundary reuses source schemas, file admission and result verification;
    /// the immutable MCP catalogue and its validator remain unchanged.
    pub async fn original716_intake(&self, name: &str, args: &Value) -> Result<Value, Error> {
        if !matches!(name, "mail_intake_ingest" | "imap_intake_poll") {
            return Err(Error::InvalidInput("unknown original716 intake"));
        }
        validate_original717(name, args)?;
        // Runtime connection decoder uses bounded u16 ports and u32 UIDs.
        if name == "imap_intake_poll"
            && (args
                .get("imapPort")
                .is_some_and(|v| v.as_u64().is_none_or(|n| n == 0 || n > 65535))
                || args
                    .get("sinceUid")
                    .is_some_and(|v| v.as_u64().is_none_or(|n| n > u32::MAX as u64)))
        {
            return Err(Error::InvalidInput("invalid IMAP port or UID"));
        }
        // original717 already implements private messages/core, transport bounds,
        // metadataPerMessage and ephemeral IMAP fields for these source schemas.
        self.original717(name, args).await
    }
}

#[cfg(test)]
mod original716_tests {
    use super::*;
    #[test]
    fn original716_intake_source_metadata_and_override_schema_remains_frozen() {
        for (name, args) in [
            (
                "mail_intake_ingest",
                json!({"company":"synthetic","source":"synthetic.eml","confirm":true,"metadataPerMessage":{"<synthetic@example.invalid>":{"invoiceNo":"SYN-1","sourceExtension":true}}}),
            ),
            (
                "imap_intake_poll",
                json!({"company":"synthetic","confirm":true,"imapHost":"imap.example.invalid","imapPort":993,"imapUsername":"synthetic","imapMailbox":"Receipts","sinceUid":0,"metadataPerMessage":{"<synthetic@example.invalid>":{"currency":"DKK"}}}),
            ),
        ] {
            assert!(validate_original717(name, &args).is_ok());
        }
        for args in [
            json!({"company":"synthetic","imapPassword":"not-admitted"}),
            json!({"company":"synthetic","imapPort":1.5}),
            json!({"company":"synthetic","metadataPerMessage":{"<x>":[]}}),
            json!({"company":"synthetic","sinceUid":-1}),
            json!({"company":"synthetic","actor":"user:forged"}),
        ] {
            assert!(validate_original717("imap_intake_poll", &args).is_err());
        }
        assert!(
            !original717_tools()
                .iter()
                .any(|v| v["name"].as_str().unwrap().contains("registrer-test"))
        );
    }
}
