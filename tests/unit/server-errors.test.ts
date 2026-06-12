// #368 — Cockpit HTTP error envelope is the SAME shape as MCP + CLI.
//
// Before this change, three Rentemester write-stacks returned three subtly
// different fejlkonvolutter:
//
//   Cockpit (old):  { ok: false, error: { code, message } }     // singular object
//   MCP:            { ok: false, errors: [string], code? }      // plural array
//   CLI:            { ok: false, errors: [string] }             // plural array
//
// An agent that drives all three could not reuse a single error parser. #368
// normalizes cockpit to the MCP/CLI shape: `{ ok: false, errors: [string], code? }`.
// The `code` enum survives at the TOP level (the discrete `bad_request`/
// `conflict`/… markers are still useful for programmatic branching), but the
// human-readable message moves into `errors[0]`.

import { describe, expect, test } from "vitest";
import { ApiError, toErrorResponse } from "../../src/server/errors";

describe("cockpit error envelope (#368)", () => {
  test("ApiError maps to {ok:false, errors:[message], code}", () => {
    const { status, body } = toErrorResponse(
      ApiError.badRequest("missing field 'foo'"),
    );
    expect(status).toBe(400);
    expect(body).toEqual({
      ok: false,
      errors: ["missing field 'foo'"],
      code: "bad_request",
    });
  });

  test("conflict ApiError preserves the 409 status and conflict code", () => {
    const { status, body } = toErrorResponse(ApiError.conflict("låst"));
    expect(status).toBe(409);
    expect(body.ok).toBe(false);
    expect(body.errors).toEqual(["låst"]);
    expect(body.code).toBe("conflict");
  });

  test("unknown error collapses to a generic internal-server-error envelope", () => {
    const { status, body } = toErrorResponse(new Error("SELECT * FROM secrets"));
    expect(status).toBe(500);
    expect(body).toEqual({
      ok: false,
      errors: ["intern serverfejl"],
      code: "internal",
    });
    // The real (potentially sensitive) message is NEVER leaked.
    expect(body.errors[0]).not.toContain("SELECT");
  });

  test("envelope has NO `error` key (the old singular shape is gone)", () => {
    const { body } = toErrorResponse(ApiError.notFound("no such company"));
    expect(body).not.toHaveProperty("error");
  });

  test("errors[] is always a non-empty array of strings", () => {
    for (const err of [
      ApiError.badRequest("x"),
      ApiError.notFound("y"),
      ApiError.conflict("z"),
      ApiError.unauthorized("w"),
      ApiError.methodNotAllowed("q"),
      new Error("leak"),
    ]) {
      const { body } = toErrorResponse(err);
      expect(Array.isArray(body.errors)).toBe(true);
      expect(body.errors.length).toBeGreaterThan(0);
      for (const e of body.errors) expect(typeof e).toBe("string");
    }
  });

  test("the unified shape is byte-identical to MCP's `errorEnvelope` output", async () => {
    // Sanity bridge: MCP's errorEnvelope produces {ok:false, errors, code?}.
    // Cockpit must produce the same shape so an agent's parser can be shared.
    const { errorEnvelope } = await import("../../src/mcp/envelope");
    const mcp = errorEnvelope(["går ikke i nul"], { code: "bad_request" });
    const { body: cockpit } = toErrorResponse(
      ApiError.badRequest("går ikke i nul"),
    );
    expect(Object.keys(cockpit).sort()).toEqual(Object.keys(mcp).sort());
    expect(cockpit.ok).toBe(mcp.ok);
    expect(cockpit.errors).toEqual(mcp.errors);
    expect(cockpit.code).toBe(mcp.code);
  });
});
