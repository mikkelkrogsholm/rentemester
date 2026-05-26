import { describe, test, expect } from "bun:test";
import { inferredMutationActor } from "../../src/cli-actor";
import { resolveActor } from "../../src/core/actor";

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

describe("resolveActor Windows support (#325)", () => {
  const origUser = process.env.USER;
  const origLogname = process.env.LOGNAME;
  const origUsername = process.env.USERNAME;
  const origRentemesterUser = process.env.RENTEMESTER_USER;
  const origRentemesterActor = process.env.RENTEMESTER_ACTOR;
  const origOpenclawAgent = process.env.OPENCLAW_AGENT;
  const origRentemesterAgent = process.env.RENTEMESTER_AGENT;

  function clearAllActorEnv() {
    delete process.env.USER;
    delete process.env.LOGNAME;
    delete process.env.USERNAME;
    delete process.env.RENTEMESTER_USER;
    delete process.env.RENTEMESTER_ACTOR;
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
    restore("RENTEMESTER_ACTOR", origRentemesterActor);
    restore("OPENCLAW_AGENT", origOpenclawAgent);
    restore("RENTEMESTER_AGENT", origRentemesterAgent);
  }

  test("resolveActor falls back to USERNAME when USER/LOGNAME are absent", () => {
    clearAllActorEnv();
    process.env.USERNAME = "WinUser";
    try {
      const ctx = resolveActor();
      expect(ctx.createdBy).toBe("user:WinUser");
    } finally {
      restoreEnv();
    }
  });

  test("resolveActor falls back to 'system' when nothing is set", () => {
    clearAllActorEnv();
    try {
      const ctx = resolveActor();
      expect(ctx.createdBy).toBe("system");
    } finally {
      restoreEnv();
    }
  });
});
