// Regression guards for the localisation sweep (#242) — covers sweep misses
// caught in code-review of commit 6618dee:
//
//   - src/core/workspace.ts:335,340,361 still threw English Error messages
//     that router.ts:1083 surfaces verbatim to the cockpit as ApiError.badRequest
//   - src/server/router.ts:1943 catch-all 404 still said "no such endpoint"
//   - src/mcp/tool-runtime.ts:94 still returned English to MCP callers
//
// Every assertion below would have failed before the fix.

import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  initWorkspace,
  renameWorkspaceCompany,
  setWorkspaceCompanyArchived,
} from "../../src/core/workspace";
import { createCompany } from "../../src/core/company";
import { resolveCompanyArg } from "../../src/mcp/tool-runtime";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";

import { cleanupDir } from "../helpers/cleanup";
function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string, companyNames: string[] = []) {
  const root = tmpRoot(label);
  initWorkspace(root);
  for (const name of companyNames) createCompany(root, { name });
  return root;
}

function config(overrides: Partial<ServerConfig> & { workspaceRoot: string }): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    ...overrides,
  };
}

async function json(cfg: ServerConfig, path: string, init?: RequestInit) {
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: (await res.json()) as { errors?: string[]; code?: string } };
}

describe("#242 sweep — workspace.ts danish error strings", () => {
  test("renameWorkspaceCompany rejects empty name in Danish", () => {
    const ws = makeWorkspace("rename-empty");
    try {
      createCompany(ws, { name: "Acme ApS" });
      expect(() =>
        renameWorkspaceCompany(ws, "acme-aps", "   "),
      ).toThrow(/firmanavn må ikke være tomt/i);
    } finally {
      cleanupDir(ws);
    }
  });

  test("renameWorkspaceCompany reports unknown slug in Danish", () => {
    const ws = makeWorkspace("rename-unknown");
    try {
      expect(() =>
        renameWorkspaceCompany(ws, "ghost", "X"),
      ).toThrow(/ingen virksomhed med slug 'ghost' findes i workspacet/);
    } finally {
      cleanupDir(ws);
    }
  });

  test("setWorkspaceCompanyArchived reports unknown slug in Danish", () => {
    const ws = makeWorkspace("archive-unknown");
    try {
      expect(() =>
        setWorkspaceCompanyArchived(ws, "ghost", true),
      ).toThrow(/ingen virksomhed med slug 'ghost' findes i workspacet/);
    } finally {
      cleanupDir(ws);
    }
  });
});

describe("#242 sweep — router catch-all 404 is in Danish", () => {
  test("an unknown /api path returns the Danish 'ukendt endpoint' body", async () => {
    const ws = makeWorkspace("api-catchall");
    try {
      const res = await json(config({ workspaceRoot: ws }), "/api/nope");
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
      expect(res.body.errors?.[0] ?? "").not.toMatch(/no such endpoint/i);
      expect(res.body.errors?.[0] ?? "").toMatch(/ukendt endpoint/i);
    } finally {
      cleanupDir(ws);
    }
  });
});

describe("#242 sweep — MCP resolveCompanyArg is in Danish", () => {
  test("an unknown slug under a configured workspace returns a Danish error", () => {
    const ws = makeWorkspace("mcp-unknown-slug");
    const prev = process.env.RENTEMESTER_WORKSPACE;
    process.env.RENTEMESTER_WORKSPACE = ws;
    try {
      const result = resolveCompanyArg("ghost");
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error).not.toMatch(/no company with slug/i);
        expect(result.error).toMatch(
          /ingen virksomhed med slug 'ghost' findes i det konfigurerede workspace/,
        );
      }
    } finally {
      if (prev === undefined) delete process.env.RENTEMESTER_WORKSPACE;
      else process.env.RENTEMESTER_WORKSPACE = prev;
      cleanupDir(ws);
    }
  });
});
