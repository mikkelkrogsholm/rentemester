import { expect, test } from "bun:test";
import { assertSettledInterceptions, cancelledNetworkRequestId, settlePausedRequest } from "../../scripts/release/cockpit-evidence-interceptions";

const options = {
  scenario: "issue-651-loading",
  requestId: "interception-job-3.0",
  networkId: "1234.3",
  url: "http://127.0.0.1:43117/api/companies/evidence-fixture/changes-since",
  cancelledNetworkRequests: new Set<string>(),
};
const invalidId = () => Promise.reject(new Error("Chrome protocol error (Fetch.fulfillRequest): Invalid InterceptionId."));

test("only records explicit Chrome cancellation with a valid Network request id", () => {
  expect(cancelledNetworkRequestId({ requestId: "1234.3", canceled: true })).toBe("1234.3");
  for (const params of [null, {}, { requestId: "1234.3", canceled: false }, { requestId: "1234.3", errorText: "net::ERR_FAILED" }, { requestId: 3, canceled: true }, { requestId: "", canceled: true }])
    expect(cancelledNetworkRequestId(params)).toBeUndefined();
});

test("accepts a completed interception", async () => {
  expect(await settlePausedRequest({ ...options, operation: async () => ({}) })).toBeUndefined();
});

test("accepts an invalid Fetch id only after cancellation of the same Network id", async () => {
  expect(await settlePausedRequest({ ...options, cancelledNetworkRequests: new Set(["1234.3"]), operation: invalidId })).toBeUndefined();
});

test("recognizes cancellation arriving while the protocol command is pending", async () => {
  const cancelledNetworkRequests = new Set<string>();
  expect(await settlePausedRequest({ ...options, cancelledNetworkRequests, operation: async () => {
    cancelledNetworkRequests.add("1234.3");
    return invalidId();
  } })).toBeUndefined();
});

test("an invalid Fetch id without exact Network cancellation still blocks the candidate", async () => {
  for (const cancelledNetworkRequests of [new Set<string>(), new Set([options.requestId]), new Set(["another-network-request"])]) {
    const failure = await settlePausedRequest({ ...options, cancelledNetworkRequests, operation: invalidId });
    expect(failure?.message).toContain("issue-651-loading interception failed:");
    expect(failure?.message).toContain(options.url);
    expect(failure?.cause).toBeInstanceOf(Error);
  }
  expect(await settlePausedRequest({ ...options, networkId: undefined, cancelledNetworkRequests: new Set([options.requestId]), operation: invalidId })).toBeInstanceOf(Error);
});

test("cancellation cannot suppress an unrelated protocol or transport failure", async () => {
  for (const message of ["CDP socket closed", "CDP timeout: Fetch.fulfillRequest", "Chrome protocol error (Fetch.fulfillRequest): Invalid parameters.", "Chrome protocol error (Page.navigate): Invalid InterceptionId."]) {
    const failure = await settlePausedRequest({ ...options, cancelledNetworkRequests: new Set(["1234.3"]), operation: () => Promise.reject(new Error(message)) });
    expect(failure?.message).toContain(message);
  }
});

test("captures a rejected interception immediately, before the later scenario drain", async () => {
  const settled = settlePausedRequest({ ...options, operation: invalidId });
  await Bun.sleep(5);
  expect(await settled).toBeInstanceOf(Error);
});

test("a replacement request added during a delayed interception remains a release blocker", async () => {
  const interceptions: Promise<Error | undefined>[] = [];
  interceptions.push((async () => {
    await Bun.sleep(5);
    interceptions.push(settlePausedRequest({ ...options, operation: invalidId }));
    return undefined;
  })());
  await expect(assertSettledInterceptions(interceptions)).rejects.toThrow("Invalid InterceptionId.");
});

test("drains a replacement that succeeds after an explicitly cancelled request", async () => {
  const interceptions: Promise<Error | undefined>[] = [];
  let replacementCompleted = false;
  interceptions.push((async () => {
    await Bun.sleep(5);
    interceptions.push((async () => {
      await Bun.sleep(5);
      replacementCompleted = true;
      return undefined;
    })());
    return settlePausedRequest({ ...options, cancelledNetworkRequests: new Set(["1234.3"]), operation: invalidId });
  })());
  await assertSettledInterceptions(interceptions);
  expect(replacementCompleted).toBe(true);
});
