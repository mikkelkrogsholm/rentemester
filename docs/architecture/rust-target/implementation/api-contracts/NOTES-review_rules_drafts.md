# Review / rules / drafts — #677 selected contracts

Source-only contract evidence, not runtime acceptance or whole-issue closure. Exactly the supplied 55 IDs; only this partition’s four flat artifacts. Existing runtime/userflow proofs are not replayed. No HEAD or live ledger probes, Cargo, server, network, provider, scanner, upload, job or authorization effects.

## Verified chain

Graphify query was performed first; its broad graph surfaced historical TypeScript relationships, so Rust handlers, global preprocessors, current authority/storage and concrete constructors were read directly. Original CLI main performs original_args normalization and original preflight **before** local_binding; local profile is consumed globally, not a leaf alias. Native remote generic guards run before the reviewed leaf. MCP Session and tools::call require objects before either native or original dispatch. SDK transport and response validator differ by family. HTTP router → credential/transport guards → worker + current Owner admission → control/service fence → storage writer/read producer was followed.

## Important non-equivalences

* Native reviewed HEAD on accounting-drafts, posting-rules and reviewed-bank refuses405 Allow:GET before credentials/body/Owner. Original draft HEAD list/show retains current authority followed by invalid_transport400. HEAD wire bodies are stripped. Existing seven guards on other routes remain unchanged. The historical R51 pre-fix finding is retained below with the R52 correction.
* Native direct HTTP derived Serde accepts full positional sequences; native CLI SDK bodies and global MCP arguments remain objects. Current nested Serde fields and stored historical JSON are distinct. Read-side payload and conditions/outcome can contain historical JSON broader than current constructors; only those fields carry arbitrary-JSON provenance.
* Native SDK throws away422 bodies. Original rule/draft SDK retains only strict known422 business receipts; 200 success receives strict producer validation. Unknown outcome and missing readback never imply rollback. Original rule dry-run HTTP remains POST but is CompanyRead, no actor/confirm and no posting authority.
* Draft native reason/journalEntryId always nullable; original projection omits absent fields. Original approve-and-post alone adds journal receipt (including empty appliedRules on durable receipt read). Show/list do not add it. Distinct author and submitter cannot review. Same-current-reviewer explicit identical approval may read durable state, never automatic replay.
* Original propose canonical-payload dedup is company/rule scoped; native propose appends a new version. Rule hash is a review fence, not a caller key. Original supports supersede/test; MCP advertises only propose/approve/explain. No fabricated original draft MCP tools.
* Queue returns up to **501** rows (not500), flags truncated when501, ordered date/id. No pagination selectors. Rules/hints never post. Control/service usage metadata may commit even when business is refused; company commit and final control disposition remain distinct.

## Schema conventions

Draft2020-12 definitions; explicit object properties/required/additionalProperties and typed arrays. CLI input is a normalized token/operation-input model, not a new JSON argv API; source-bound notes retain public split/attached preprocessing and original human output. MCP result includes JSON-RPC envelope and text+structuredContent correlation. JSON Schema cannot assert string-equals-serialized-structuredContent, source hashes, relational authority or exact decimal lexical conversion; those obligations are stated in operation contracts, not waived. HTTP GET/show query models are documentation objects, not request bodies. No provider approval is claimed.

## Validation scope

Existing strict Python oracle is the structural/source-hash check. Main independently reviews semantic correctness. Full #677 advertised-operation coverage, all55 scope, J01–J07, final installed manual desktop acceptance, release/cutover/retirement remain with main.

Executed checks: `python3 /tmp/r51-677-oracle.py review_rules_drafts` passed (55 exact IDs, source hashes and schema resolution); `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"` completed (34009 nodes, 107681 edges). These are structural/source observations only, not semantic acceptance or live workflow proof.

Lead source review: pre-fix HEAD accounting-drafts and reviewed-bank can return200 and commit draft/source/audit rows under valid body/current CompanyDraftWrite; no journal posting. Their HEAD wire body is zero; NativeDraftOutput is only the constructor schema. Posting-rules HEAD uses empty action and refuses before any lifecycle/audit INSERT. These pre-fix source facts require a method guard before external publication; no effect probe was run.

Lead protocol review binds each static RPC message to its actual Session/globalguard/localparse code; a cartesian code/message enum is not promised.

R52 producer correction supersedes the R51 pre-fix HEAD snapshot: the shared native handler now refuses accounting-drafts, posting-rules and reviewed-bank HEAD with405 Allow:GET before proof/body/Owner. No native HEAD creates drafts. The original draft list/show HEAD authorization-then400 behavior is unchanged. Existing realserver fixture covers20HEAD cases on10routes plus GET/POST auth and ledger/account/company/audit/artifact invariants. No authenticated financial/provider effects are replayed.
