import { normalizeObservedRequestPaths } from "./cockpit-evidence";

const OWNED_REQUEST_POLL_INTERVAL_MS = 100;

export type ExactOwnedRequestWaitOptions = {
  scenario: string;
  base: string;
  expectedPaths: string[];
  observedUrls: () => string[];
  deadlineMs: number;
  now?: () => number;
  sleep?: (milliseconds: number) => Promise<void>;
};

/**
 * Wait for every exact scenario-owned request to reach Fetch.requestPaused.
 * This intentionally observes only declared URLs; final comparison remains exact.
 */
export async function waitForExactOwnedRequests(
  options: ExactOwnedRequestWaitOptions,
): Promise<void> {
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? ((milliseconds) => Bun.sleep(milliseconds));
  const expectedUrls = options.expectedPaths.map((path) => `${options.base}${path}`);
  const deadline = now() + options.deadlineMs;
  let observed = options.observedUrls();
  let missing = () => options.expectedPaths.filter((_, index) => !observed.includes(expectedUrls[index]!));
  while (missing().length) {
    const remaining = deadline - now();
    if (remaining <= 0) {
      throw new Error(
        `exact owned requests were not observed before deadline: scenario=${options.scenario}; missing=${JSON.stringify(missing())}; observed=${JSON.stringify(normalizeObservedRequestPaths(observed))}`,
      );
    }
    await sleep(Math.min(OWNED_REQUEST_POLL_INTERVAL_MS, remaining));
    observed = options.observedUrls();
  }
}
