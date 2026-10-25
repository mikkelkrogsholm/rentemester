# R116: Original MCP first use on the common Rust server

Issue #717; epic #672 and all 55 original children retain their full scope.
Current accepted source is `fe6c48d341369c0cd6e396688702702a8b84754f`.
This increment completes the missing original `company_add` and
`bookkeeping_workbench` adapters. It does not declare the other original tools,
all 31 workflows, release or cutover complete. The installed desktop test stays last.

## Fixed public contract

Use the exact original schemas, annotations and envelopes in
`docs/architecture/rust-target/implementation/baseline/contracts-mcp.json` and
`baseline/inventory.json`, and verify handlers in the referenced TypeScript source.
Existing native tools and their bodies remain supported. No new accounting engine,
client calculations, inferred chart, actor override or automatic write retry.

- `company_add` accepts original flat workspace/name/slug/CVR/fiscal fields and
  optional confirmation. Missing/false confirmation produces the original tool
  error envelope with `CONFIRM_REQUIRED`, before workspace resolution or HTTP.
  Convert the original integer month to the existing native string month. Call
  the existing common `POST /api/companies` once; preserve current WorkspaceManage,
  separate ledger, explicit creator grant, duplicate refusal and uncertain outcome.
  Success is source `data:{slug,name}`. Bind a supplied absolute workspace to a
  genuinely verified local profile; never ignore it or invent a remote path.
  Preserve explicit unqualified variants as limitations, not full compatibility.
- `bookkeeping_workbench` accepts the original flat company/from/to and every
  optional filter, pagination and search field. Use the existing canonical GET
  workbench producer and CompanyRead. Preserve population blockers separately
  from selected/page data; no persistence, writes or guessed readiness. Return
  original `data:{workbench:...}` and exact read-only/idempotent/closed-world hints.
- Original text JSON and structuredContent must be identical, with isError driven
  by the original outer ok. Preserve errors/code/rules and business versus parse
  failures. Follow existing original adapters rather than the generic native wrapper.
- Session/permission/expiry checks always use current trusted server authority.
  Caller workspace/company/filter strings never confer access or select another
  backend. Reconcile an uncertain create through existing reads, never repeat it.

## Parallel ownership

Company owner: client `src/company_creation.rs`, client `tests/company_creation.rs`,
MCP `src/company_creation.rs`, MCP `src/tools.rs`, MCP `tests/stdio.rs` and new
`tests/stdio/company_creation_original116.rs`. The shared tools/test roots have
this one writer. Add only required company routing/module registration there.
Export `company_creation::call_original(api,args) -> Result<Value,&'static str>`
as an already wrapped original tool result and route company_add before native
wrapping. Existing create_company stays unchanged.

Workbench owner: client `src/bookkeeping_batch.rs`, client
`tests/bookkeeping_batch.rs`, MCP `src/bookkeeping_batch.rs`, MCP
`tests/stdio/bookkeeping_batch.rs`. Add the name through the existing tools()
and is_original()/call_original() route; do not edit shared roots. Keep all
Group182 batch contracts, independent approval, source/effect guards and native
batch behavior intact. SDK methods can be defined on ApiClient in owned modules;
no shared lib export changes or lockfile changes are needed.

Both owners are not alone, must not revert other work or delegate. One commit
per worker, `Work-Issue: #717`. Read Graphify first. Read this relative file from
your checkout; the filesystem tool cannot read arbitrary external contracts.
Do not run provider, registry, browser, production or financial actions.

## Observable acceptance

Write bounded existing-style SDK and MCP stdio cases for all original fields,
source envelopes/hints, missing confirmation before network, wrong selector,
current refusal, response-loss/no replay and inert filtered workbench reads.
New MCP cases use the test-name prefix `original_first_use116_` so zero-case
execution is detectable. Syntax checks alone are not execution acceptance.

Workers run Rust syntax checks and awaited Graphify AST extraction into their
own SGD_BUILD_DIR. Do not start cold Rust builds in worker targets: the observed
host has about 36 GiB free and the unchanged guard requires 30 GiB. The lead
reviews actual diffs and runs the new SDK/stdio cases and retained native tests
on the integrated source using the existing idle warm target through cargo-guard.
No new dependency installs, global compiler lock, lowered disk gate or test bypass.
