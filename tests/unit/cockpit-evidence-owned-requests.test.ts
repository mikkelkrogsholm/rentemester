import { expect, test } from "bun:test";
import { waitForExactOwnedRequests } from "../../scripts/release/cockpit-evidence-owned-requests";

const base = "http://127.0.0.1:43117";

test("waits for a secondary exact owned request that follows a completed URL outcome", async () => {
  const observed = [`${base}/api/companies/evidence-fixture/party-hub`];
  let elapsed = 0;
  await waitForExactOwnedRequests({
    scenario: "issue-653-normal-desktop",
    base,
    expectedPaths: [
      "/api/companies/evidence-fixture/party-hub",
      "/api/companies/evidence-fixture/party-hub/party-evidence",
    ],
    observedUrls: () => observed,
    deadlineMs: 5_000,
    now: () => elapsed,
    sleep: async (milliseconds) => {
      elapsed += milliseconds;
      if (elapsed === 200)
        observed.push(`${base}/api/companies/evidence-fixture/party-hub/party-evidence`);
    },
  });
  expect(elapsed).toBe(200);
});

test("fails at the bounded deadline with missing exact paths and observed normalized paths", async () => {
  const observed = [`${base}/api/companies/evidence-fixture/party-hub?tab=all`];
  let elapsed = 0;
  await expect(waitForExactOwnedRequests({
    scenario: "issue-653-normal-desktop",
    base,
    expectedPaths: ["/api/companies/evidence-fixture/party-hub/party-evidence"],
    observedUrls: () => observed,
    deadlineMs: 200,
    now: () => elapsed,
    sleep: async (milliseconds) => { elapsed += milliseconds; },
  })).rejects.toThrow(
    'scenario=issue-653-normal-desktop; missing=["/api/companies/evidence-fixture/party-hub/party-evidence"]; observed=["/api/companies/evidence-fixture/party-hub?tab=all"]',
  );
  expect(elapsed).toBe(200);
});
