// Per-company financial-statement views for the cockpit (#320).
//
// Split out of `server/data.ts` by #320. The year-aware Overblik dashboard and
// the four core statement views — Resultatopgørelse, Balance, Saldobalance and
// the Flerårsoversigt — each computed from the posted ledger via
// `core/financial-statements`, or from the #197 archive for an archived year.
//
// Archive classification uses the shared `classifyAccountSection` (#321), the
// same rule the live balance sheet applies, so the archive-aware views never
// disagree with the live ones. Behaviour is unchanged from the pre-split
// `server/data.ts`. Money is kroner throughout.
//
// This file is now a thin barrel — each of the five builders lives in its own
// module under `./statements/`. The public surface is unchanged: every
// previously-exported name remains reachable via this path.

export * from "./statements/_shared";
export * from "./statements/overview";
export * from "./statements/income-statement";
export * from "./statements/balance";
export * from "./statements/trial-balance";
export * from "./statements/multi-year";
