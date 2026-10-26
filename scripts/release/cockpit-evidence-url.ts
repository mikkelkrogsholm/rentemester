/** Compare browser navigation outcomes without discarding a required query. */
export function browserUrlMatchesExpected(actual: URL, expected: string) {
  const target = new URL(expected, actual.origin);
  return target.search ? actual.pathname + actual.search === target.pathname + target.search : actual.pathname === target.pathname;
}
