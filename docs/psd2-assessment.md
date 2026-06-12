# PSD2 / open banking — assessment and deferral

**Decision:** deferred. PSD2 integration is not "easy to add" by any
reasonable definition; the current CSV-import path covers the same need
deterministically and the marginal value of automating it does not pay for
the regulatory + ongoing-operations cost yet.

## What PSD2 actually requires

To pull bank transactions directly from a Danish bank into Rentemester, the
process is — at the minimum — a four-party dance:

1. **The bank** — Danske Bank, Nordea, Jyske, Lunar etc. — exposes an
   account-information API behind a PSD2 endpoint. Each bank has its own
   onboarding, contract terms, and rate-limit policy.
2. **A licensed TPP** (third-party provider) operates the connection. Either
   you become a TPP (which requires an FSA licence — eIDAS QWAC + QSeal
   certificates, a published security policy, and a successful sandbox audit
   — months of work) **or** you integrate with an aggregator that already
   holds the licence: Nordigen / GoCardless Bank Account Data, Tink, Yapily,
   Plaid (EU), Salt Edge.
3. **The end user** consents in a bank-hosted redirect flow (NemID/MitID for
   Danish banks). The consent is time-bounded (typically 90 days, capped at
   180 days by EBA RTS), so the integration also has to handle re-consent
   prompts on a recurring basis.
4. **Rentemester** maps the aggregator's normalised transaction format to its
   own `bank_transactions` shape, persists the refresh tokens securely, and
   replays through `importBankCsv` / suggest-matches / reconcile.

None of those four steps are trivial. The lightest realistic slice — single
bank, Nordigen free tier, no refresh-token UX yet — is conservatively a
week's work + ongoing maintenance as each Danish bank's PSD2 surface
changes.

## Why the CSV path still wins for now

- **Deterministic and auditable.** A CSV from the bank's own export is the
  bank's word; importing it leaves a clear, replayable trail.
- **No live secrets.** The PSD2 path requires Rentemester to handle bank
  consent tokens that grant ongoing read access. A self-hosted product
  multiplies the surface area significantly.
- **Bank coverage is uneven.** Lunar has a friendly API; Danske / Nordea /
  Jyske rely on the PSD2 sandbox-then-production process via an aggregator.
  A half-finished integration that only works for one bank would be more
  confusing than the consistent CSV path.
- **Friction is real but bounded.** The current bank-import modal (and the
  `bank import` CLI) takes a CSV in one click; there is no daily friction,
  only a per-period one.

## When to revisit

Revisit when one or more of the following is true:

1. **Concrete user friction.** A beta user runs into the CSV export
   workflow as a daily blocker — not a monthly annoyance.
2. **Nordigen-class aggregator becomes a no-cost commodity.** Bank Account
   Data has been free in tiers; if that changes (or a Danish-bank-friendly
   alternative emerges with a permissive free tier), the cost equation
   tilts.
3. **Lunar-only audience.** If the early users are concentrated on Lunar,
   a Lunar-only direct-API slice is a much smaller engineering item and a
   reasonable first step that does not require an aggregator at all.

When that day comes the slice will need: token storage with rotation,
a re-consent CLI/UI ping, a parser per aggregator that lands rows through
the existing `importBankCsv` deterministic pipeline (NOT a parallel path),
and per-bank regression fixtures.

## Status in the ROADMAP

`ROADMAP.md` lists PSD2 as a next-but-not-now item; this document is the
reasoning behind that placement. It is not a "no", it is a "not yet, and
here is what would have to change for it to be yes."
