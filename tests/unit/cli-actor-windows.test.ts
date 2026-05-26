import { describe, test, expect } from "bun:test";
import { inferredMutationActor } from "../../src/cli-actor";

describe("inferredMutationActor Windows support (#325)", () => {
  // Save originals so we can restore after each test
  const origUser = process.env.USER;
  const origLogname = process.env.LOGNAME;
  const origUsername = process.env.USERNAME;
  const origRentemesterUser = process.env.RENTEMESTER_USER;
  const origOpenclawAgent = process.env.OPENCLAW_AGENT;
  const origRentemesterAgent = process.env.RENTEMESTER_AGENT;

  function clearAllActorEnv() {
    delete process.env.USER;
    delete process.env.LOGNAME;
    delete process.env.USERNAME;
    delete process.env.RENTEMESTER_USER;
    delete process.env.OPENCLAW_AGENT;
    delete process.env.RENTEMESTER_AGENT;
  }

  function restoreEnv() {
    const restore = (key: string, value: string | undefined) => {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    };
    restore("USER", origUser);
    restore("LOGNAME", origLogname);
    restore("USERNAME", origUsername);
    restore("RENTEMESTER_USER", origRentemesterUser);
    restore("OPENCLAW_AGENT", origOpenclawAgent);
    restore("RENTEMESTER_AGENT", origRentemesterAgent);
  }

  test("falls back to USERNAME when USER and LOGNAME are absent (Windows)", () => {
    clearAllActorEnv();
    process.env.USERNAME = "WindowsUser";
    try {
      const actor = inferredMutationActor();
      expect(actor).toBe("user:WindowsUser");
    } finally {
      restoreEnv();
    }
  });

  test("prefers USER over USERNAME when both are set", () => {
    clearAllActorEnv();
    process.env.USER = "UnixUser";
    process.env.USERNAME = "WindowsUser";
    try {
      const actor = inferredMutationActor();
      expect(actor).toBe("user:UnixUser");
    } finally {
      restoreEnv();
    }
  });

  test("returns null when no actor env vars are set (negative case)", () => {
    clearAllActorEnv();
    try {
      const actor = inferredMutationActor();
      expect(actor).toBeNull();
    } finally {
      restoreEnv();
    }
  });
});
