# #677 — payables / supplier commitments

64 frozen selected identities in this partition (207 across the four worker families). Prior 419 accepted operations / 18 partitions remain unchanged. This is source-only contract evidence, not whole-#677 acceptance, live provider approval, release, cutover, retirement, or final desktop acceptance. Main owns integration and semantic acceptance; all55/J01–J07 remain retained.

## Evidence and boundaries

Graphify query was performed first, followed by current Rust route/application/storage/SDK/CLI/MCP callchain reading. No Cargo, server, browser, network, HEAD probe, financial operation, provider, upload, scanner, authentication, or accepted runtime proof replay was performed. Draft2020-12 schemas and exact operation rows are confined to the four owned files. Local refs target this partition or accepted partitions only.

## Important source findings for main review

* Native payable HEAD has the supplied early405 / Allow: GET guard before credentials/body/Owner. It was not probed or changed.
* **Native supplier commitment HEAD is not safe GET semantics:** its handler sets `post = method != GET`, takes mutation origin proof and JSON body, then chooses fallback `Apply`, including `/matches`. A valid admitted confirmed approval can append an event while HEAD suppresses response bytes. These rows deliberately retain the observed source semantics, not a HEAD-safety claim. Main must review this bounded route risk before acceptance; worker did not change production or execute it.
* Original payable HEAD reaches read credential/selection, then rejects non-GET with400 invalid_query. Original supplier HEAD carries invalid transport through held current-authority admission, then rejects400 invalid_input. These are not standardized to native payable405.
* All public MCP calls reject non-object arguments globally before dispatch; leaf Serde tuple support is not public MCP grammar. Direct typed HTTP bodies retain declaration-order positional sequences, Option slots, custom default-tail omission, and custom optional rejection of explicit null. SDK body guards require objects.
* Native CLI/MCP retain nested core data. Original payable CLI lifts successful `data` to human/JSON output; original payable MCP preserves its original envelope. Original supplier MCP removes core ok/errors and wraps the remaining result under data. Native transports discard422 bodies; original payable also uses that generic transport. Original supplier SDK uniquely retains exact valid422 `{ok:false,errors}` bodies and treats write5xx/408 as uncertain.
* Supplier plan/approval/change/match are planning/evidence, not authority to create payables, journals, settlement, messages, or provider payments. Plan hash binds source/vendor/prior revision; original change resolves current approved revision inside the held transaction while native change requires expectedPayloadHash.
* Approval keys are company event keys, **not principal-scoped generic receipts**. Existing key lookup compares requestedInput/id/hash before new plan. Current source and authority checks still precede it. No approval TTL/tombstone is invented. Original payable keys instead use ledger/operation/principal-kind/subject scope, retained30-day outcomes and durable tombstones. Original payable CLI exposes no key. Match natural digest dedup is distinct from automatic retry.
* Native supplier historical payload readers parse stored JSON Values. Field-specific stored-value definitions preserve that broader read grammar; they do not widen current creation input or top-level response envelopes. Original supplier SDK validates stricter projections/proposal shape than a raw Value reader. Money is numeric on current producers, not inferred decimal strings; nullable stored/output fields retain null.

## Global guards, authority, uncertainty

CLI main local_binding consumes global split/attached --local-profile before leaf dispatch, validates private profile/exact origin/private Unix binding and has no TCP fallback. Native remote flags themselves use split values only. Service/report/source/budget guards precede native leaves; recognition of a flag is not permission to ignore it. Original CLI has its own parse/usage/human/JSON0/1/2 stream behavior.

Current Owner/source admission and retirement, current credential/membership/company capability, backup governance, workspace transfer, accounting period/current rule/source hash and immutable document evidence remain required. Caller actor is an equality assertion only. Payables use actual original creditor/open-item and bank mapping evidence; no overpayment, duplicate recognition or bank-row reuse. Original chain verifies bank source bytes/journals before mutation and receipt lookup.

Company business SQL, usage/control metadata, and transport delivery are different stages. Planning/read/refusal can still involve metadata writes. Missing result is not rollback; changed authority is not proof of no effect. Reconcile canonical payables/bank/journal/document or commitments/history/matches/source evidence before another deliberate action. No automatic retry, generated keys, fresh-key replay permission, or live provider guarantee is claimed.

## Validation

Run the supplied existing strict oracle from this worker root:

`python3 /tmp/r53-677-oracle.py payables_supplier`

Its structural result does not replace main's independent constructor/diff/source semantic review. The final worker response records the actual result, not a fabricated pass. Graphify AST extraction is directed to the independent SGD build output, not owned documentation or the common graph.

Lead final review correction: workerfinal already wires recursive nested Http constructors; retain them. Native service payable read usage can be not_committed or committed, distinct from committed business read. Existing native payableHEAD is405 before anyquery/body/currentauthority; two supplierHEAD inputs/constructors explicitly retain observedpre-fixApply until batchedmethodfix beforeexternalpublication. No financial/provider replay.
