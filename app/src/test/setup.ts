// Bun test setup — adds jest-dom matchers and resets DOM/mocks between
// tests so each spec runs against a clean slate.
import * as matchers from "@testing-library/jest-dom/matchers";
import { afterEach, beforeEach, expect, vi } from "bun:test";
import { cleanup } from "@testing-library/react";
import { restoreGlobals, stubGlobal } from "./globals";

expect.extend(matchers);

let consoleProblems: string[] = [];

beforeEach(() => {
  consoleProblems = [];
  localStorage.clear();
  sessionStorage.clear();
  vi.spyOn(console, "warn").mockImplementation((...args) => {
    consoleProblems.push(args.map(String).join(" "));
  });
  vi.spyOn(console, "error").mockImplementation((...args) => {
    consoleProblems.push(args.map(String).join(" "));
  });
  stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.replace(/^https?:\/\/[^/]+/, "").split("?")[0] === "/api/health") {
      return new Response(JSON.stringify({
        ok: true, service: "rentemester-cockpit", workspace: "/test", authRequired: false,
        deploymentProfile: "local", build: {}, provenance: {}, routes: [],
      }), { headers: { "content-type": "application/json" } });
    }
    throw new Error(`Unexpected fetch in cockpit test: ${String(input)}`);
  }));
});

afterEach(() => {
  cleanup();
  if (consoleProblems.length > 0) {
    throw new Error(`Unexpected console output in cockpit test:\n${consoleProblems.join("\n")}`);
  }
  vi.restoreAllMocks();
  restoreGlobals();
});
