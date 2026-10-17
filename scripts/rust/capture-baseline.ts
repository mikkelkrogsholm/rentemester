import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { join, relative, resolve } from "node:path";
const root = resolve(import.meta.dir, "../..");
const output = process.argv[2];
const currentBacklog = process.argv[3];
if (!output)
  throw new Error(
    "Usage: bun scripts/rust/capture-baseline.ts OUTPUT.json [CURRENT-ISSUES.json]",
  );
const { COMMAND_SPECS, flagsForSpec, SIDE_EFFECTING_COMMANDS } = await import(
  root + "/src/cli-meta.ts"
);
const { MUTATING_COMMANDS } = await import(root + "/src/cli-actor.ts");
const { ROUTE_CATALOG } = await import(root + "/src/server/router.ts");
const { registerAllTools } = await import(root + "/src/mcp/registry.ts");
const { COMPANY_ROUTE_DEFINITIONS, companyRoutePattern } = await import(
  root + "/app/src/company-navigation.ts"
);
const { RETRY_CLASS_BY_OPERATION, RETRY_OPERATION_NAMES } = await import(
  root + "/src/core/idempotency.ts"
);
const { z } = await import(root + "/node_modules/zod/index.js");
const ts = (await import(root + "/node_modules/typescript/lib/typescript.js"))
  .default;
const read = (p: string) => readFileSync(join(root, p), "utf8");
const sha = (v: string) => createHash("sha256").update(v).digest("hex");
const walk = (p: string): string[] =>
  readdirSync(join(root, p))
    .sort()
    .flatMap((n) =>
      statSync(join(root, p, n)).isDirectory()
        ? walk(p + "/" + n)
        : [p + "/" + n],
    );
const csv = read("docs/architecture/rust-target/epic/source-coverage.csv")
  .trim()
  .split(/\r?\n/)
  .slice(1)
  .map((l) => {
    let [path, owner, kind, hash] = l.split(",");
    return { path, owner, kind, hash };
  });
const owner = (p: string) => csv.find((r) => r.path === p)?.owner ?? "UNKNOWN";
const sources = new Set<string>();
const ev = (paths: string[]) =>
  paths.filter(Boolean).map((path) => {
    sources.add(path);
    return { path, sha256: sha(read(path)) };
  });
const unknown = {
  status: "UNKNOWN",
  reason:
    "Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required.",
};
const retry = (name: string, readonly: boolean) => {
  if (RETRY_CLASS_BY_OPERATION[name])
    return {
      classification: RETRY_CLASS_BY_OPERATION[name],
      basis: "src/core/idempotency.ts:RETRY_CLASS_BY_OPERATION",
    };
  for (let [k, v] of Object.entries(RETRY_OPERATION_NAMES))
    if ((v as Set<string>).has(name))
      return {
        classification: (
          {
            keyIdempotent: "key-idempotent",
            naturalIdempotent: "natural-idempotent",
            externalProviderReconciled: "external-provider-reconciled",
            naturalIdempotentCli: "natural-idempotent",
          } as any
        )[k],
        basis: "src/core/idempotency.ts:RETRY_OPERATION_NAMES." + k,
      };
  return readonly
    ? {
        classification: "safe-read",
        basis:
          "INFERRED from read-only registry classification; not independently behavior-verified",
      }
    : {
        classification: "UNKNOWN",
        basis:
          "No exact operation entry in exposed retry registry. Do not infer safe retries from annotation alone.",
      };
};
function schema(shape: any) {
  try {
    return z.toJSONSchema(shape?.safeParse ? shape : z.object(shape), {
      unrepresentable: "any",
    });
  } catch (e) {
    return { ...unknown, reason: String(e) };
  }
}
const cliSourceByKey = new Map<string, string>();
for (const p of walk("src/cli-meta").filter((p) => p.endsWith(".ts"))) {
  const ast = ts.createSourceFile(p, read(p), ts.ScriptTarget.Latest, true);
  function visit(n: any) {
    if (
      ts.isPropertyAssignment(n) &&
      n.name.getText(ast) === "key" &&
      ts.isStringLiteral(n.initializer)
    )
      cliSourceByKey.set(n.initializer.text, p);
    ts.forEachChild(n, visit);
  }
  visit(ast);
}
const cli = COMMAND_SPECS.map((s: any) => {
  const f = cliSourceByKey.get(s.key);
  const mutation = MUTATING_COMMANDS.has(s.key),
    side = SIDE_EFFECTING_COMMANDS.has(s.key);
  return {
    ...s,
    acceptedFlags: [...new Set([...s.allowedFlags, ...flagsForSpec(s)])],
    mutation,
    sideEffecting: side,
    classification: mutation
      ? "actor-gated-mutation"
      : side
        ? "side-effecting"
        : "read",
    migrationOwner: "R44",
    inputSchema: unknown,
    successSchema: unknown,
    refusalSchema: unknown,
    parseFailureSchema: unknown,
    retry: retry(s.key, !mutation && !side),
    evidence: ev([
      "src/cli-meta.ts",
      "src/cli-meta/_shared.ts",
      "src/cli-actor.ts",
      ...(f ? [f] : []),
    ]),
  };
});
const mcp: any[] = [];
registerAllTools({
  registerTool(name: string, config: any) {
    const stack = new Error().stack ?? "";
    const files = [...stack.matchAll(/\(([^()\n]+\.ts):\d+:\d+\)/g)]
      .map((m) => relative(root, m[1]))
      .filter((p) => !p.startsWith(".."));
    mcp.push({
      name,
      title: config.title,
      description: config.description,
      annotations: config.annotations,
      inputSchema: schema(config.inputSchema),
      outputSchema: config.outputSchema ? schema(config.outputSchema) : unknown,
      exactSuccessDataSchema: unknown,
      refusalSchema: unknown,
      protocolFailureSchema: unknown,
      retry: retry(name, config.annotations?.readOnlyHint === true),
      migrationOwner: "R45",
      evidence: ev([...new Set(["src/mcp/registry.ts", ...files])]),
    });
    return {};
  },
} as any);
const http = ROUTE_CATALOG.map((r: any) => ({
  ...r,
  migrationOwner: "R42",
  inputSchema: unknown,
  successSchema: unknown,
  refusalSchema: unknown,
  protocolFailureSchema: unknown,
  retry:
    r.effect === "read"
      ? retry("", true)
      : {
          classification: "UNKNOWN",
          basis:
            "HTTP catalog exposes effect/permission, no exact retry identity",
        },
  evidence: ev(["src/server/router.ts"]),
}));
const app = read("app/src/App.tsx");
const companyElements = Object.fromEntries(
  [...app.matchAll(/^\s*(?:"([^"]+)"|([\w-]+)):\s*<([A-Za-z]+View)/gm)].map(
    (m) => [m[1] ?? m[2], m[3]],
  ),
);
const browser = COMPANY_ROUTE_DEFINITIONS.map((r: any) => ({
  ...r,
  path: companyRoutePattern(r.segment),
  view: companyElements[r.id] ?? "UNKNOWN",
  migrationOwner: "R46",
  schema: unknown,
  evidence: ev(["app/src/App.tsx", "app/src/company-navigation.ts"]),
}));
const staticRoutes = [
  ...app.matchAll(/<Route\s+path="([^"]+)"\s+element=\{([\s\S]*?)\}\s*\/>/g),
].map((m) => ({
  path: m[1],
  elementSource: m[2],
  migrationOwner: "R46",
  schema: unknown,
  evidence: ev(["app/src/App.tsx"]),
}));
const core: any[] = [];
for (const p of walk("src/core").filter(
  (p) => p.endsWith(".ts") && !/\.test\.ts$/.test(p),
)) {
  let text = read(p),
    ast = ts.createSourceFile(
      p,
      text,
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.TS,
    );
  for (let node of ast.statements) {
    if (
      !node.modifiers?.some((m: any) => m.kind === ts.SyntaxKind.ExportKeyword)
    )
      continue;
    if (ts.isFunctionDeclaration(node) && node.name)
      core.push({
        name: node.name.text,
        path: p,
        line: ast.getLineAndCharacterOfPosition(node.getStart()).line + 1,
        signatureSource: text
          .slice(node.getStart(), node.body?.pos ?? node.end)
          .trim(),
        migrationOwner: owner(p),
        status: "source-exported; not behavior-verified",
        wireSchema: unknown,
        evidence: ev([p]),
      });
  }
}
const special = csv
  .filter(
    (r) =>
      r.kind === "provider" ||
      r.kind === "job" ||
      /provider|runner|recurring|scheduler|digisense|imap|delivery|scanner|extractor/.test(
        r.path,
      ),
  )
  .map((r) => ({
    ...r,
    sha256: sha(read(r.path)),
    currentMatchesPlanningHash: sha(read(r.path)) === r.hash,
    evidence: ev([r.path]),
    status:
      "source module present; credentials/procurement/runtime availability UNKNOWN",
  }));
const migrations = walk("src/core")
  .filter((p) => /\/(workspace-)?migrations\/.*\.json$/.test(p))
  .map((p) => ({
    path: p,
    metadata: JSON.parse(read(p)),
    migrationOwner: owner(p) === "UNKNOWN" ? "R12" : owner(p),
    evidence: ev([p]),
  }));
const backlog = JSON.parse(
  read("docs/architecture/rust-target/epic/existing-issues.snapshot.json"),
);
const plan = JSON.parse(read("docs/architecture/rust-target/epic/plan.json"));
const coverage = csv.map((r) => ({
  ...r,
  currentSha256: sha(read(r.path)),
  matchesPlanningSha256: sha(read(r.path)) === r.hash,
}));
const out = {
  format: "rentemester-rust-operation-baseline-draft/1",
  revision: {
    head: Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root })
      .stdout.toString()
      .trim(),
    branch: Bun.spawnSync(["git", "branch", "--show-current"], { cwd: root })
      .stdout.toString()
      .trim(),
    trackedDiffSha256: sha(
      Bun.spawnSync(["git", "diff", "HEAD", "--", "."], {
        cwd: root,
      }).stdout.toString(),
    ),
    note: "Source evidence hashes bind worktree content; HEAD alone does not bind dirty files. Read-only registry capture; no handlers invoked.",
  },
  cli,
  mcp,
  http,
  browserCompany: browser,
  browserStatic: staticRoutes,
  coreExportedFunctions: core,
  providerJobCandidateModules: special,
  migrations,
  retryRegistry: RETRY_CLASS_BY_OPERATION,
  sourceCoverage: coverage,
  currentExistingBacklog: currentBacklog
    ? {
        basis:
          "Caller-supplied GitHub issue snapshot; only planning-snapshot IDs retained",
        snapshotSha256: sha(readFileSync(currentBacklog, "utf8")),
        issues: JSON.parse(readFileSync(currentBacklog, "utf8"))
          .filter((i: any) => backlog.some((b: any) => b.number === i.number))
          .sort((a: any, b: any) => a.number - b.number),
      }
    : {
        status: "UNKNOWN",
        reason:
          "No current GitHub snapshot supplied; use an explicit read-only gh capture.",
      },
  existingBacklog: {
    basis: "Local planning snapshot, not a freshly queried GitHub status",
    snapshotSha256: sha(
      read("docs/architecture/rust-target/epic/existing-issues.snapshot.json"),
    ),
    issues: backlog,
  },
  migrationIssues: plan.issues.map((i: any) => ({
    id: i.id,
    title: i.title,
    sourcePaths: i.source_paths,
  })),
  sharedContracts: {
    cli: {
      exitCodes: {
        0: "success",
        1: "business/ledger refusal",
        2: "parse/usage error",
      },
      stdout:
        "UNKNOWN per operation; docs/cli-contract.md describes global contract; exact failure streams need fixture capture",
      evidence: ev(["docs/cli-contract.md"]),
    },
    mcp: {
      text: "JSON.stringify(envelope)",
      structuredContent: "same envelope object",
      isError: "!envelope.ok",
      evidence: ev(["src/mcp/envelope.ts"]),
    },
  },
  evidence: [...sources]
    .sort()
    .map((path) => ({ path, sha256: sha(read(path)) })),
  limitations: [
    "Draft inventory only; does not establish R01 completion.",
    "All exact per-operation HTTP/CLI schemas, error casing/nullability/streams, and MCP data payloads require explicit contracts and fixture verification.",
    "Core export inventory includes exported function declarations; exported arrow functions, object methods, classes, internal operations, schema-defined triggers and re-exports require a later pass.",
    "Browser static routes include wildcard and authentication gates; access conditions remain in App.tsx element source and are not semantically evaluated.",
    "Provider/job candidates are source-coverage/path-derived modules; no live credentials, procurement, scheduling or runtime availability verified.",
    "Owner R44/R45/R42/R46 is interface delivery ownership; domain ownership must be traced from core source coverage, not guessed from operation names.",
    "Original planning status retained separately; currentExistingBacklog is the fresh read-only GitHub snapshot and must be rebased before cutover.",
    "Registry presence indicates exposed capability, not implemented/complete runtime behavior.",
    "No per-operation positive oracle or negative fixture mapping yet.",
  ],
};
writeFileSync(output, JSON.stringify(out, null, 2) + "\n");
const counts = {
  cli: cli.length,
  mcp: mcp.length,
  http: http.length,
  browserCompany: browser.length,
  browserStatic: staticRoutes.length,
  coreFunctions: core.length,
  specialModules: special.length,
  migrations: migrations.length,
  sourceCoverage: coverage.length,
  drift: coverage.filter((r) => !r.matchesPlanningSha256).length,
};
writeFileSync(
  output + ".report.md",
  "# Rust baseline capture (draft)\n\n" +
    JSON.stringify(counts, null, 2) +
    "\n\n" +
    out.limitations.map((v) => "- " + v).join("\n") +
    "\n",
);
console.log(counts);
