/** Shared assertions used by the actual synthetic differential/fault harnesses. */
import assert from "node:assert/strict";

export function assertCanonicalEqual(actual: unknown, expected: unknown): void {
  assert.deepEqual(actual, expected);
}

/** A refusal/rollback must both refuse the effect and preserve every table. */
export function assertNotCommitted(outcome: string, before: string, after: string): void {
  assert.equal(outcome, "not_committed", "expected refusal/rollback, received an accepted or uncertain effect");
  assertCanonicalEqual(after, before);
}
