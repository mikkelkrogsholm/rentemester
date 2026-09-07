import { expect, test } from "bun:test";
import {
  LOADING_HEADING_READY_DEADLINE_MS,
  NORMAL_HEADING_READY_DEADLINE_MS,
  sanitizedBodyExcerpt,
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
