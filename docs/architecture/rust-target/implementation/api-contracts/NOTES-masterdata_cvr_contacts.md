# Master data, company profile, CVR, VIES and contacts — #677

Source-only contracts. No provider calls, runtime effects, builds or accepted-proof replays. Prior 34 partitions and all retained release/cutover scope remain unchanged. Main owns independent semantic review and integration; this is not issue closure.

Schemas distinguish native direct HTTP Serde sequences from outer-object-only SDK/MCP inputs. Native profile/patch map handlers accept objects only. HEAD company and original master-data handlers refuse 405 Allow: GET before credentials/body/Owner; CVR lookup/status retain actual GET-handler execution on HEAD, with Axum stripping the response body, including possible lookup/cache effects.

## Transport and recovery review

- Global protocol mismatch is **409 protocol_incompatible**, before handler authentication. Native handlers emit application/json and cache-control:no-store; protocol middleware emits its own JSON response.
- MCP Session requires initialize then notifications/initialized. IDs are UTF-8 strings up to256bytes or signed i64/unsignedu64; malformed framing uses -32700/id:null. tools::call rejects non-object arguments before every selected native/original dispatch. Import_contacts remains normal1MiB frame despite12MiB HTTP body.
- CLI global local_binding consumes split/attached --local-profile and admits only private Unix/exact-origin binding, without TCP fallback. Native remote argv and normalized flags differ. Original help/output normalization runs before transport; original human stderr and JSON stdout differ from native compact stdout errors. Earlier remote report/source/service/CFO/budget guards remain effective.
- Native raw HTTP producer IDs are i64. Original SDK read projections enforce positive safeinteger IDs and terms, and exact ordered envelopes; they are not evidence that raw producer IDs are JavaScript-safe. CVR cached producer deserializes the typed CvrCompany before returning it; shareCapital is numeric/null, not arbitrary stored JSON. No arbitrary top-level data schema is used.
- Original master-data transport retains only strictly valid422 businessfalse; other refusal bodies are discarded. Native generic request discards422; import_contacts has its own businessfalse retention path. Native CVR lookup/sync may return HTTP200 outer ok:true with inner ok:false. Original fromCvr failure may commit cache independently before contact creation.
- VIES explicitly contacts trusted operator provider on every selected validation, including within90-day cache TTL. CVR uses30-day company cache and may use stale cache on provider failure. No live approval was exercised. Creation initialization, control owner grant and catalog publication are distinct stages; missing publication response does not roll them back.

## Validation evidence

`python3 /tmp/r58-677-oracle.py masterdata_cvr_contacts` passed exact62selected identities and current source hashes with strict referenced-schema checks. This is structural/source-bound validation, not independent semantic acceptance. Graphify extract completed in isolated SGD_BUILD_DIR/graphify (code-only, no clustering). No Cargo, network/provider, server, browser or mutation replay was run. Only the four owned flat files are delivered; prior1285operations/34partitions and all55/J01–J07/finaldesktop/release/cutover/retirement scope remain unchanged.
