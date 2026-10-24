# Budget and forecast contracts — existing #677

## Delivered scope

33 frozen selected operation IDs, all eight contract dimensions, Draft 2020-12
local `$defs`, current-byte SHA-256 constructor bindings, and no unresolved rows.
Only the four owned flat artifacts are written. Previously accepted 626 operations
in 22 partitions remain unchanged. This is not acceptance of whole #677 or a waiver
of the retained 55-family, J01–J07, final desktop, release, cutover or retirement scope.
Main retains review, integration and acceptance.

## Source distinctions preserved

- Graphify query `budget forecast` preceded source inspection. Graph observations
  were navigation, not authoritative wire constructors; actual Rust files bind rows.
- Three native HEAD routes fence early with 405 / Allow GET before credentials,
  body parsing or Owner access. Four original HEAD routes still run the original
  GET handler/read and Axum suppresses response bytes. No runtime HEAD replay.
- Direct HTTP native Set accepts object or Serde sequence in declaration order
  `accountNo, period, amount, notes`. Original HTTP Write order is
  `confirm, accountNo, period, amount, notes`; default tail omissions differ from
  explicit null amount (custom number deserializer rejects null). Body limits are
  UTF-8 byte limits, not Unicode character counts. SDK JSON body validators admit
  objects only; MCP's global object guard prevents public tuple arguments.
- Native reads use exact live fiscal labels, including shifted fiscal years and
  years discovered from budget revisions. Original list/range is calendar-month
  based; original monthly and weekly cash forecasts have different result shapes.
- `budget_vs_actual` is one MCP overload row: presence of either `from` or `to`
  dispatches original first, before native Serde. Original and native tool result
  envelopes differ: original lifts `ok/errors/appliedRules` and wraps only the
  remaining nonempty core; native wraps the entire HTTP envelope in `data`.
- Original dedicated transport retains validated HTTP 422 business-false output;
  invalid 422 is refused. Native generic request discards non-2xx bodies and does
  not validate successful domain JSON. Producer schemas describe actual output,
  not an invented generic arbitrary top-level SDK data object.
- RPC errors enumerate code/message pairs with selected reachable leaf parser
  messages. String IDs are bounded to 256 UTF-8 bytes, numeric IDs retain signed
  i64 / unsigned u64 range, and framing errors carry null ID. Startup argv
  diagnostics are not RPC errors; unreachable post-validation helper errors are
  not advertised as public selected-tool errors.
- Original CLI preflight occurs before global local binding. Local profile may
  be split or attached; after validation it is consumed, exact private Unix
  transport installed, and synthetic server flags appended. No TCP fallback.
  Original flags accept attached values; native remote leaf flags do not acquire
  those aliases. Original JSON/human stdout/stderr and exit 0/1/2 remain distinct
  from native remote JSON output. Parsed `--actor` is not silently ignored: the
  original payload guard rejects it.

## Financial authority and recovery

Native set requires current CompanyAdmin human authority; original set uses
CompanyDraftWrite and supports the actual service path; reads require CompanyRead.
Current session/membership, held physical Owner namespace, schema/rules,
retirement, transfer and backup controls remain authoritative. Service usage/control
commit and company business commit are separate stages; refusals and advisory reads
must not be generalized to zero metadata effects.

Every accepted set appends a new immutable revision and audit record, even for an
identical account/month/amount. There is no caller-key, CAS, deduplication TTL,
tombstone or automatic retry promise. Latest MAX(id) projection is not replay
protection. After uncertainty inspect canonical account/month budget and audit;
missing response or absence is not rollback and changed authority is not no-effect.
No file publication, posting, journal generation or provider approval is claimed.

Dimension comparisons preserve account context, classified/unallocated amounts,
other members, exact evidence notes and null unreviewed budget/variance. Reviewed
sets must reconcile the latest account budget; stale assignment source proof is
reported unallocated. Full dimension definition/assignment/budget-plan/apply is a
necessary separate later family, not delivered implicitly by these reads.

Monthly forecast aligns cash windows to calendar-month starts (1–18 months).
Weekly forecast starts on the supplied date (1–13 weeks, omission defaults 13).
Observed base cash is separated from scenario assumptions, undated account budgets,
FX exclusions and VAT settlement status unknown. Forecasts never generate recurring
invoices, post journals or execute providers. No company-specific data/examples.

## Validation

`python3 /tmp/r55-677-oracle.py budget_forecast`:
PASS exactselected budget_forecast 33 resolved sourcebound schema contracts;
semanticreview required; whole677 remains open.

Only existing strict Python/source checks are used. No Cargo/server/browser/network,
authentication, financial, upload, scanner, job or external-provider effects run.
Independent AST extraction is written to `$SGD_BUILD_DIR/graphify`, outside owned code.
