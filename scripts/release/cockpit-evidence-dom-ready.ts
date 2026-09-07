export const NORMAL_HEADING_READY_DEADLINE_MS = 10_000;
export const LOADING_HEADING_READY_DEADLINE_MS = 2_000;
export const HEADING_READY_POLL_INTERVAL_MS = 100;

export type HeadingReadySnapshot = {
  headingVisible: boolean;
  url: string;
  readyState: string;
  bodyText: string;
};

export type HeadingReadyOptions = {
  scenario: string;
  state: "normal" | "loading" | "empty" | "warning-or-blocked" | "error";
  probe: () => Promise<HeadingReadySnapshot>;
  now?: () => number;
  sleep?: (milliseconds: number) => Promise<void>;
};

export function headingReadyDeadline(state: HeadingReadyOptions["state"]): number {
  return state === "loading"
    ? LOADING_HEADING_READY_DEADLINE_MS
    : NORMAL_HEADING_READY_DEADLINE_MS;
}

export function sanitizedBodyExcerpt(bodyText: string): string {
  return bodyText.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim().slice(0, 280);
}

function timeoutError(scenario: string, snapshot: HeadingReadySnapshot): Error {
  return new Error(
    `scenario page-local heading did not become visible before deadline: scenario=${scenario}; url=${snapshot.url}; readyState=${snapshot.readyState}; bodyText=${JSON.stringify(sanitizedBodyExcerpt(snapshot.bodyText))}`,
  );
}

/** Bounded readiness gate: it proves the page shell mounted before DOM evidence begins. */
export async function waitForScenarioHeading(options: HeadingReadyOptions): Promise<void> {
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? ((milliseconds) => Bun.sleep(milliseconds));
  const deadline = now() + headingReadyDeadline(options.state);
  let snapshot = await options.probe();
  while (!snapshot.headingVisible) {
    const remaining = deadline - now();
    if (remaining <= 0) throw timeoutError(options.scenario, snapshot);
    await sleep(Math.min(HEADING_READY_POLL_INTERVAL_MS, remaining));
    snapshot = await options.probe();
  }
}
