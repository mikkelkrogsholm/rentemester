import { expect, test } from "bun:test";
import {
  LOADING_HEADING_READY_DEADLINE_MS,
  NORMAL_HEADING_READY_DEADLINE_MS,
  POST_ACTION_CONDITION_DEADLINE_MS,
  sanitizedBodyExcerpt,
  waitForBoundedCondition,
  waitForScenarioHeading,
} from "../../scripts/release/cockpit-evidence-dom-ready";

const hidden = {
  headingVisible: false,
  url: "http://127.0.0.1:43117/companies/evidence-fixture/opmaerksomhed",
  readyState: "interactive",
  bodyText: "\u0000  Loading\n  evidence\tpage ",
};

test("retries the bounded page-local heading probe until React has mounted", async () => {
  let probes = 0;
  let elapsed = 0;
  await waitForScenarioHeading({
    scenario: "issue-649-normal-desktop",
    state: "normal",
    probe: async () => ({ ...hidden, headingVisible: ++probes === 3 }),
    now: () => elapsed,
    sleep: async (milliseconds) => { elapsed += milliseconds; },
  });
  expect(probes).toBe(3);
  expect(elapsed).toBe(200);
});

test("uses a strict shorter mount deadline for loading evidence", () => {
  expect(LOADING_HEADING_READY_DEADLINE_MS).toBe(2_000);
  expect(NORMAL_HEADING_READY_DEADLINE_MS).toBe(10_000);
});

test("waits for a delayed post-keyboard condition without a static sleep", async () => {
  let probes = 0;
  let elapsed = 0;
  await waitForBoundedCondition({
    scenario: "issue-649-normal-desktop",
    expectedOutcome: "URL matching /companies/evidence-fixture/opmaerksomhed?filter=open",
    deadlineMs: POST_ACTION_CONDITION_DEADLINE_MS,
    probe: async () => ({
      satisfied: ++probes === 3,
      url: "http://127.0.0.1:43117/companies/evidence-fixture/opmaerksomhed?filter=open",
      bodyText: "Attention items",
    }),
    now: () => elapsed,
    sleep: async (milliseconds) => { elapsed += milliseconds; },
  });
  expect(probes).toBe(3);
  expect(elapsed).toBe(200);
});

test("reports scenario, expected outcome, URL and visible body on post-keyboard timeout", async () => {
  let elapsed = 0;
  const snapshot = {
    satisfied: false,
    url: "http://127.0.0.1:43117/companies/evidence-fixture/opmaerksomhed",
    bodyText: "\u0000  Attention\n  still loading ",
  };
  await expect(waitForBoundedCondition({
    scenario: "issue-649-normal-desktop",
    expectedOutcome: "URL matching /companies/evidence-fixture/opmaerksomhed?filter=open",
    deadlineMs: 200,
    probe: async () => snapshot,
    now: () => elapsed,
    sleep: async (milliseconds) => { elapsed += milliseconds; },
  })).rejects.toThrow(
    "scenario=issue-649-normal-desktop; expected=URL matching /companies/evidence-fixture/opmaerksomhed?filter=open; url=http://127.0.0.1:43117/companies/evidence-fixture/opmaerksomhed; bodyText=\"Attention still loading\"",
  );
  expect(elapsed).toBe(200);
});

test("reports actionable sanitized diagnostics on a heading timeout", async () => {
  let elapsed = 0;
  try {
    await waitForScenarioHeading({
      scenario: "issue-649-normal-desktop",
      state: "normal",
      probe: async () => hidden,
      now: () => elapsed,
      sleep: async (milliseconds) => { elapsed += milliseconds; },
    });
    throw new Error("expected heading readiness timeout");
  } catch (error) {
    expect(error).toBeInstanceOf(Error);
    expect((error as Error).message).toContain("scenario=issue-649-normal-desktop");
    expect((error as Error).message).toContain(`url=${hidden.url}`);
    expect((error as Error).message).toContain("readyState=interactive");
    expect((error as Error).message).toContain('bodyText="Loading evidence page"');
  }
  expect(elapsed).toBe(NORMAL_HEADING_READY_DEADLINE_MS);
  expect(sanitizedBodyExcerpt("\u0000  Loading\n  evidence\tpage ")).toBe("Loading evidence page");
});
