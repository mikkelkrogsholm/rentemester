export const NORMAL_HEADING_READY_DEADLINE_MS = 10_000;
export const LOADING_HEADING_READY_DEADLINE_MS = 2_000;
export const HEADING_READY_POLL_INTERVAL_MS = 100;
export const POST_ACTION_CONDITION_DEADLINE_MS = 5_000;

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

export type BoundedConditionSnapshot = {
  satisfied: boolean;
  url: string;
  bodyText: string;
  readyState?: string;
};

export type BoundedConditionOptions = {
  scenario: string;
  expectedOutcome: string;
  deadlineMs: number;
  probe: () => Promise<BoundedConditionSnapshot>;
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

function conditionTimeoutError(
  options: BoundedConditionOptions,
  snapshot: BoundedConditionSnapshot,
): Error {
  return new Error(
    `scenario condition did not become true before deadline: scenario=${options.scenario}; expected=${options.expectedOutcome}; url=${snapshot.url};${snapshot.readyState ? ` readyState=${snapshot.readyState};` : ""} bodyText=${JSON.stringify(sanitizedBodyExcerpt(snapshot.bodyText))}`,
  );
}

/** Poll a browser-visible condition without hiding a delayed UI transition behind a static sleep. */
export async function waitForBoundedCondition(options: BoundedConditionOptions): Promise<void> {
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? ((milliseconds) => Bun.sleep(milliseconds));
  const deadline = now() + options.deadlineMs;
  let snapshot = await options.probe();
  while (!snapshot.satisfied) {
    const remaining = deadline - now();
    if (remaining <= 0) throw conditionTimeoutError(options, snapshot);
    await sleep(Math.min(HEADING_READY_POLL_INTERVAL_MS, remaining));
    snapshot = await options.probe();
  }
}

/** Bounded readiness gate: it proves the page shell mounted before DOM evidence begins. */
export async function waitForScenarioHeading(options: HeadingReadyOptions): Promise<void> {
  await waitForBoundedCondition({
    scenario: options.scenario,
    expectedOutcome: "page-local heading visible",
    deadlineMs: headingReadyDeadline(options.state),
    probe: async () => {
      const snapshot = await options.probe();
      return { ...snapshot, satisfied: snapshot.headingVisible };
    },
    now: options.now,
    sleep: options.sleep,
  });
}
