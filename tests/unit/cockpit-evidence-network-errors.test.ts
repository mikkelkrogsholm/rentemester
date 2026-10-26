import { expect, test } from "bun:test";
import { expectedNetworkError } from "../../scripts/release/cockpit-evidence-network-errors";

const base = "http://127.0.0.1:43117";
const request = (status: 200 | 403 | 500) => ({ urlPattern: "/api/companies/evidence-fixture/attention", status });
const log = (url: string, status: number, overrides: Record<string, unknown> = {}) => ({
  entry: {
    source: "network",
    level: "error",
    url,
    text: `Failed to load resource: the server responded with a status of ${status} ()`,
    ...overrides,
  },
});

test("records exact owned Chrome network 403 and 500 errors as expected evidence", () => {
  for (const status of [403, 500] as const)
    expect(expectedNetworkError(log(`${base}${request(status).urlPattern}`, status), base, [request(status)])).toEqual({
      url: `${base}${request(status).urlPattern}`,
      status,
      source: "network",
      level: "error",
      message: `Failed to load resource: the server responded with a status of ${status} ()`,
    });
});

test("keeps unrelated, partial, wrong-status and normal endpoint failures fatal", () => {
  expect(expectedNetworkError(log(`${base}/api/companies/other`, 403), base, [request(403)])).toBeUndefined();
  expect(expectedNetworkError(log(`${base}${request(403).urlPattern}?extra=1`, 403), base, [request(403)])).toBeUndefined();
  expect(expectedNetworkError(log(`${base}${request(403).urlPattern}`, 500), base, [request(403)])).toBeUndefined();
  expect(expectedNetworkError(log(`${base}${request(200).urlPattern}`, 200), base, [request(200)])).toBeUndefined();
});

test("keeps JavaScript exceptions fatal", () => {
  expect(expectedNetworkError({ exceptionDetails: { text: "Uncaught Error" } }, base, [request(403)])).toBeUndefined();
});
