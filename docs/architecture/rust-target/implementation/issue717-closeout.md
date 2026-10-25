# #717 — whole agent-interface acceptance

Accepted source: `7e01053ac60bb21753d334ea50376a4ded9148d7`.

The common Rust server now exposes all 214 original MCP names with their source argument and result contracts. The nine missing backup/restore, intake, email and configuration operations are implemented. Source GDPR, backup governance and DigiSense projections use the existing Owner and current authority checks. Audited native GDPR export has a distinct alias from the source read-only audit operation.

The original 31 workflows and 26 capabilities resolve against actual registration. Missing or mismatched CLI/HTTP references remain explicitly unavailable; registration does not grant permission or provider readiness.

All agreed combined checks passed: protocol manifest; 37 library, 4 server, 10 SDK original, 16 MCP dispatcher, 5 discovery, 10 original stdio, 2 extended SDK, 4 extended MCP and 202 general stdio tests. The full test commands, revision, log hashes, preserved failures and lead review are in [issue717-evidence.json](issue717-evidence.json). Separate scoped current-authority and physical readback evidence is retained there.

The existing strict 38-partition contract oracle covers the actual 689 HTTP, 533 CLI and 448 MCP operations. All prior 1629 operation identities and prior schema definitions remain; only the reviewed source/effect contracts and current source pins changed.

No live company, financial or provider effect was used as acceptance. The original built-in SMTP dry-run boundary remains. Real-data cutover and final installed desktop acceptance belong to their existing remaining issues. Acceptance of #717 does not accept the entire rewrite.

The round stops before another issue. It closed concrete contract gaps through parallel bounded deliveries and small lead repairs. The cost came from the breadth of the original interface issue and stale protocol fixtures; future closure trials should choose a smaller whole existing issue, without reducing its acceptance.
