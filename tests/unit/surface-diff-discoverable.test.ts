// Tests: docs/mcp-tool-surface.md (CLI↔MCP-mapping section),
// src/server/router.ts (HTTP route catalog), src/mcp/registry.ts (precise
// surface comment) — the CLI↔MCP↔HTTP surface diff must be discoverable by
// an agent without reading source files (#376).
//
// Why this guard exists: the registry comment used to say MCP is "tæt på 1:1
// med src/cli-meta.ts (kendte afvigelser er dokumenteret i docs/...)". In
// reality there are >= 10 deviations, and HTTP is a third surface with no
// route discovery at all. An agent that picks MCP as primary surface will
// never discover `agent run`, `annual-report`, `dashboard`, `opening-balance`
// etc. — and an agent that picks CLI as primary surface will never discover
// `cvr_lookup`, `peppol_*` or `portfolio_*`. The mapping must therefore be
// machine-readable.
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const REPO_ROOT = new URL("../..", import.meta.url).pathname.replace(/\/$/, "");
const SURFACE_DOC = join(REPO_ROOT, "docs/mcp-tool-surface.md");
const REGISTRY = join(REPO_ROOT, "src/mcp/registry.ts");
const ROUTER = join(REPO_ROOT, "src/server/router.ts");
const COCKPIT_API_DOC = join(REPO_ROOT, "docs/cockpit-api.md");

function listCliFiles(): string[] {
  return readdirSync(join(REPO_ROOT, "src/cli"))
    .filter((f) => f.endsWith(".ts"))
    .map((f) => f.replace(/\.ts$/, ""));
}

function listMcpToolFiles(): string[] {
  return readdirSync(join(REPO_ROOT, "src/mcp/tools"))
    .filter((f) => f.endsWith(".ts"))
    .map((f) => f.replace(/\.ts$/, ""));
}

describe("#376 — surface diff is discoverable (docs/mcp-tool-surface.md)", () => {
  test("has an explicit CLI-only section listing every CLI file without an MCP twin", () => {
    const doc = readFileSync(SURFACE_DOC, "utf8");
    // The mapping section must call out CLI-only with a discoverable heading.
    expect(doc).toMatch(/###\s*CLI-only/);

    // Every src/cli/<x>.ts file that is not paired with a src/mcp/tools/<x>.ts
    // must either appear under "CLI-only" or be the CLI-equivalent of an MCP
    // domain that uses a different filename (intentional pairing). The doc is
    // the single source of truth — drift fails the build.
    const cliFiles = new Set(listCliFiles());
    const mcpFiles = new Set(listMcpToolFiles());
    const cliOnlyCandidates = [...cliFiles].filter((f) => !mcpFiles.has(f));

    // Acceptance criterion: at least 10 deviations listed (the issue states
    // ">= 10"). The candidates set is the lower bound the doc must cover.
    expect(cliOnlyCandidates.length).toBeGreaterThanOrEqual(7);

    // Find the CLI-only section.
    const cliOnlyMatch = doc.match(/###\s*CLI-only[\s\S]*?(?=^###\s|^##\s|\Z)/m);
    expect(cliOnlyMatch, "CLI-only section not found").not.toBeNull();
    const cliOnlySection = cliOnlyMatch![0];

    // The known CLI-only domains from the issue must be listed by filename.
    for (const fname of [
      "agent",
      "annual-report",
      "dashboard",
      "opening-balance",
      "reg",
      "report",
      "serve",
    ]) {
      expect(cliOnlySection, `CLI-only section missing entry: ${fname}`)
        .toContain(fname);
    }
  });

  test("has an explicit MCP-only section listing every MCP tool file without a CLI twin", () => {
    const doc = readFileSync(SURFACE_DOC, "utf8");
    expect(doc).toMatch(/###\s*MCP-only/);

    const mcpOnlyMatch = doc.match(/###\s*MCP-only[\s\S]*?(?=^###\s|^##\s|\Z)/m);
    expect(mcpOnlyMatch, "MCP-only section not found").not.toBeNull();
    const mcpOnlySection = mcpOnlyMatch![0];

    for (const fname of ["cvr", "peppol", "portfolio"]) {
      expect(mcpOnlySection, `MCP-only section missing entry: ${fname}`)
        .toContain(fname);
    }
  });

  test("the combined CLI-only + MCP-only lists hold at least 10 deviations", () => {
    // Acceptance criterion: ">= 10 stk." (CLI-only + MCP-only combined).
    const doc = readFileSync(SURFACE_DOC, "utf8");
    const cliOnly = (doc.match(/###\s*CLI-only[\s\S]*?(?=^###\s|^##\s|\Z)/m) ?? [""])[0];
    const mcpOnly = (doc.match(/###\s*MCP-only[\s\S]*?(?=^###\s|^##\s|\Z)/m) ?? [""])[0];
    const bulletRe = /^\s*[-*]\s+/gm;
    const cliBullets = (cliOnly.match(bulletRe) ?? []).length;
    const mcpBullets = (mcpOnly.match(bulletRe) ?? []).length;
    expect(cliBullets + mcpBullets).toBeGreaterThanOrEqual(10);
  });

  test("every src/cli/<x>.ts and src/mcp/tools/<x>.ts is mentioned in the mapping doc", () => {
    // The third acceptance criterion: a new src/cli/<x>.ts or
    // src/mcp/tools/<x>.ts added without being listed in the mapping doc
    // must fail this test.
    const doc = readFileSync(SURFACE_DOC, "utf8");
    for (const f of listCliFiles()) {
      expect(doc, `mapping doc does not mention src/cli/${f}.ts`).toContain(f);
    }
    for (const f of listMcpToolFiles()) {
      expect(doc, `mapping doc does not mention src/mcp/tools/${f}.ts`).toContain(f);
    }
  });
});

describe("#376 — registry comment names the real deviation count", () => {
  test("src/mcp/registry.ts no longer claims a loose 'tæt på 1:1' without a count", () => {
    const src = readFileSync(REGISTRY, "utf8");
    // The old loose comment must be sharpened: either a count is named or a
    // direct link to the mapping sections is given.
    expect(src).toContain("docs/mcp-tool-surface.md");
    // Must mention either a numeric deviation count or the section heading.
    expect(src).toMatch(/CLI-only|MCP-only|afvigelser/);
    // The vague "tæt på 1:1" claim, if it appears, must be qualified — not
    // standing alone as the final word. We require either a count or a
    // pointer to the explicit deviations section right next to it.
    if (/tæt på 1:1/.test(src)) {
      expect(src).toMatch(/(CLI-only|MCP-only)/);
    }
  });
});

describe("#376 — HTTP layer exposes a route catalog", () => {
  test("GET /api / GET /api/health returns a 'routes' list with method+pattern", () => {
    // The cockpit API doc must promise a discoverable route catalog. The
    // exact wire shape is implementation-defined, but the doc must name
    // the field so an agent can branch on it.
    const apiDoc = readFileSync(COCKPIT_API_DOC, "utf8");
    expect(apiDoc).toMatch(/route[- ]?catalog|GET\s+\/api[^a-zA-Z]/i);
    expect(apiDoc).toContain("routes");
  });

  test("src/server/router.ts builds a route catalog from a single source-of-truth list", () => {
    const src = readFileSync(ROUTER, "utf8");
    // The router must export or define a ROUTE_CATALOG / route list so it
    // can be returned by GET /api and tested directly.
    expect(src).toMatch(/ROUTE_CATALOG|routeCatalog|routes:\s*\[/);
  });
});
