import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
const candidate = readFileSync(
  join(root, ".github", "workflows", "release-candidate.yml"),
  "utf8",
);
const promote = readFileSync(
  join(root, ".github", "workflows", "release-promote.yml"),
  "utf8",
);
const www = readFileSync(join(root, ".github", "workflows", "www.yml"), "utf8");
const testWorkflow = readFileSync(join(root, ".github", "workflows", "test.yml"), "utf8");
const smokeWorkflow = readFileSync(join(root, ".github", "workflows", "smoke.yml"), "utf8");
const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
  scripts: Record<string, string>;
};
const appPackageJson = JSON.parse(readFileSync(join(root, "app", "package.json"), "utf8")) as {
  scripts: Record<string, string>;
};
const registryStatus = readFileSync(
  join(root, "scripts", "release", "registry-manifest-status.ts"),
  "utf8",
);
const containerTest = readFileSync(
  join(root, "scripts", "release", "test-local-container.ts"),
  "utf8",
);

describe("release workflow security contract", () => {
  test("pins every privileged third-party action to a full commit", () => {
    for (const workflow of [candidate, promote, www, testWorkflow, smokeWorkflow]) {
      const uses = [...workflow.matchAll(/^\s*uses:\s*([^\s#]+)/gm)].map(
        (match) => match[1],
      );
      expect(uses.length).toBeGreaterThan(0);
      expect(uses.every((value) => /@[0-9a-f]{40}$/i.test(value))).toBe(true);
    }
  });

  test("keeps full suites local while GitHub builds the cockpit", () => {
    expect(packageJson.scripts["test:parallel"]).toBe("bun test --parallel=4 --timeout=20000 --only-failures");
    expect(appPackageJson.scripts["test:parallel"]).toBe("bun test --parallel --timeout=20000 --only-failures");
    expect(packageJson.scripts["verify:local"]).toContain("bun run test:parallel");
    expect(packageJson.scripts["verify:local"]).toContain("bun run cockpit:test:parallel");
    expect(testWorkflow).not.toContain("run: bun test\n");
    expect(testWorkflow).not.toContain("run: bun run cockpit:test");
    expect(testWorkflow).toContain("run: bun run cockpit:build");
    expect(candidate).not.toContain("run: bun test");
    expect(candidate).not.toContain("run: bun run cockpit:test");
    expect(candidate).not.toContain("run: bun run smoke");
    expect(candidate).toContain("run: bun run cockpit:build");
    expect(testWorkflow.match(/bun install --frozen-lockfile/g)).toHaveLength(3);
  });

  test("makes the strict runtime typecheck a merge and release gate", () => {
    expect(testWorkflow).toContain("name: Typecheck runtime and release scripts");
    expect(testWorkflow).toContain("run: bun run typecheck:runtime");
    expect(candidate).toContain("name: Typecheck runtime and release scripts");
    expect(candidate.indexOf("run: bun run typecheck:runtime")).toBeLessThan(
      candidate.indexOf("name: Build and push candidate image"),
    );
  });

  test("makes the pinned Biome lint policy a merge and release gate", () => {
    expect(testWorkflow).toContain("name: Lint TypeScript and React sources");
    expect(testWorkflow).toContain("run: bun run lint");
    expect(candidate).toContain("name: Lint TypeScript and React sources");
    expect(candidate.indexOf("run: bun run lint")).toBeLessThan(
      candidate.indexOf("name: Build and push candidate image"),
    );
  });

  test("makes the production license allowlist a merge and release gate", () => {
    expect(testWorkflow).toContain("run: bun run supply-chain:licenses");
    expect(candidate).toContain("run: bun run supply-chain:licenses");
  });

  test("blocks advisories and binds audit, licenses and lockfile into release evidence", () => {
    expect(testWorkflow).toContain("run: bun run supply-chain:audit");
    expect(candidate).toContain("run: bun audit --audit-level=low --json > supply-chain-audit.json");
    expect(candidate).toContain("SUPPLY_CHAIN_AUDIT_REPORT_PATH: supply-chain-audit.json");
    expect(candidate).toContain("bun run supply-chain:evidence > supply-chain-evidence.json");
    expect(candidate).toContain("RELEASE_SUPPLY_CHAIN_SHA256");
    expect(candidate).toContain("RELEASE_AGENT_DISCOVERY_SHA256");
    expect(candidate).toContain("Verify packaged agent-discovery coverage identity");
    expect(candidate).toContain("agent-discovery-coverage-report.json");
    expect(candidate).toContain("run src/agent-discovery-coverage-cli.ts > agent-discovery-coverage-report.json");
    expect(candidate).toContain('RENTEMESTER_AGENT_DISCOVERY_IMAGE_DIGEST="$IMAGE_DIGEST"');
    expect(candidate).not.toContain("/app/src/agent-discovery-coverage-report.json");
    expect(candidate).toContain("supply-chain-evidence.json.sha256");
    expect(promote).toContain("candidate supply-chain report does not match approved evidence");
    expect(promote).toContain("supply-chain-evidence.json.sha256");
    expect(promote).toContain("agent-discovery-coverage-report.json.sha256");
    expect(promote).toContain("candidate agent-discovery coverage does not match release evidence");
    expect(promote).toContain("candidate/agent-discovery-coverage-report.json.sha256");
    expect(promote).toContain("run src/agent-discovery-coverage-cli.ts > /tmp/rentemester-agent-discovery-coverage.json");
    expect(promote).toContain("cmp /tmp/rentemester-agent-discovery-coverage.json candidate/agent-discovery-coverage-report.json");
  });

  test("uses the required smoke check only for image build and runtime verification", () => {
    expect(smokeWorkflow).toContain("run: bun run container:test");
    expect(smokeWorkflow).toContain("run: bun run container:reproducibility");
    expect(smokeWorkflow).not.toContain("run: bun run smoke");
    expect(smokeWorkflow).not.toContain("run: bun test");
  });

  test("publishes evidence only after the candidate image is attested", () => {
    expect(candidate).toContain('if [ "$commit" != "$GITHUB_SHA" ]');
    expect(candidate.indexOf("name: Attest candidate image provenance")).toBeGreaterThan(
      candidate.indexOf("name: Build and push candidate image"),
    );
    expect(candidate.indexOf("name: Upload candidate evidence for Digisense")).toBeGreaterThan(
      candidate.indexOf("name: Attest candidate image provenance"),
    );
    expect(candidate).toContain("name: release-candidate-evidence");
    expect(candidate).toContain("Review status: **not reviewed by Digisense**");
    expect(candidate).toContain("Immutable digest:");
    expect(candidate).toContain("docker pull $REGISTRY_IMAGE@$DIGEST");
    expect(candidate).not.toContain("Approved bytes:");
    expect(candidate).not.toContain(":latest");
  });

  test("smokes the published digest before attestation and evidence", () => {
    const reproducibilityGate = candidate.indexOf("name: Verify reproducible OCI export");
    const readinessGate = candidate.indexOf(
      "name: Verify container readiness and persisted restart",
    );
    const imageBuild = candidate.indexOf("name: Build and push candidate image");
    const containerSmoke = candidate.indexOf("name: Smoke the published candidate digest");
    const attestation = candidate.indexOf("name: Attest candidate image provenance");
    expect(reproducibilityGate).toBeGreaterThan(0);
    expect(readinessGate).toBeGreaterThan(reproducibilityGate);
    expect(imageBuild).toBeGreaterThan(readinessGate);
    expect(containerSmoke).toBeGreaterThan(imageBuild);
    expect(attestation).toBeGreaterThan(containerSmoke);
    expect(candidate).toContain('image="$REGISTRY_IMAGE@$IMAGE_DIGEST"');
    expect(candidate).toContain('test "$(id -u)" = 1000');
    expect(candidate).toContain("/workspace:rw,nosuid,size=64m,uid=1000,gid=1000");
    expect(candidate).toContain("serve --workspace /workspace --host 0.0.0.0 --port 4319");
    expect(candidate).not.toContain("serve --workspace /workspace --host 127.0.0.1");
    expect(candidate).toContain("cockpit asset missing");
    expect(candidate).toContain("fetch(new URL(asset,base))");
    expect(candidate).toContain("name: Render the published local-container cockpit");
    expect(candidate).toContain('"/companies/release-check?year=2026"');
    expect(candidate).toContain('"/companies/release-check/manage?source=release-candidate"');
    expect(candidate).toContain('company add \\');
    expect(candidate).toContain('--workspace /workspace --name "Release Check" --slug release-check');
    expect(candidate).toContain("--cvr 12345678");
    expect(candidate).toContain("--virtual-time-budget=5000 --dump-dom \"$base_url$route\"");
    expect(candidate).toContain("<div id=\"root\"><div class=\"app-shell\"");
    expect(candidate).toContain("grep -F 'Release Check'");
    expect(candidate).toContain("local-container cockpit was rejected by the UI security-profile gate");
    expect(candidate).toContain("sbom: true");
    expect(candidate).toContain("outputs: type=registry,rewrite-timestamp=true");
    expect(candidate).not.toMatch(/^\s+push:\s+true\s*$/m);
    expect(candidate).toContain("image=moby/buildkit:buildx-stable-1@sha256:28a898719c18a33f4e8000685287fa36fd0dd9560c6440227d3a732d79bb41d8");
    expect(candidate).toContain("run: bun run container:reproducibility");
    expect(candidate).toContain("run: bun run container:test");
    expect(containerTest).toContain('"--network", "none"');
    expect(containerTest).toContain('"--memory", "512m"');
    expect(containerTest).toContain('"--pids-limit", "128"');
    expect(containerTest).toContain("syntheticTextPdf()");
    expect(containerTest).toContain("syntheticNoTextPdf()");
    expect(containerTest).toContain("PDF parse cache was not reused");
    expect(containerTest).toContain("verified PDF text/layout missing");
    expect(candidate.match(/RELEASE_VERSION: \$\{\{ steps\.identity\.outputs\.version \}\}/g)?.length).toBeGreaterThanOrEqual(2);
    expect(candidate.match(/RELEASE_GIT_COMMIT: \$\{\{ steps\.identity\.outputs\.commit \}\}/g)?.length).toBeGreaterThanOrEqual(2);
    expect(candidate.match(/SOURCE_DATE_EPOCH: \$\{\{ steps\.identity\.outputs\.source_date_epoch \}\}/g)?.length).toBeGreaterThanOrEqual(2);
    expect(candidate).toContain("EXPECTED_BUN_VERSION");
    expect(candidate).toContain("EXPECTED_BASE_IMAGE_DIGEST");
    expect(candidate).toContain('org.opencontainers.image.base.digest');
    expect(candidate).toContain('test "$(bun --version)" = "$EXPECTED_BUN_VERSION"');
    expect(candidate).toContain("documents ingest --example");
    expect(candidate).toContain("documents ingest example invalid");
    expect(candidate).toContain("SPDX SBOM");
    expect(candidate).toContain("Extract digest-bound SPDX SBOM evidence");
    expect(candidate).toContain("const sbom=value.SPDX??value");
    expect(candidate).toContain("sbom.spdx.json.sha256");
    expect(candidate).toContain("SBOM must contain pdfjs-dist@6.2.108");
  });

  test("binds promotion to one successful trusted run and its attestation", () => {
    for (const required of [
      '.github/workflows/release-candidate.yml',
      'workflow_dispatch',
      'head_branch',
      'conclusion',
      'run_attempt',
      'name: release-candidate-evidence',
      'sha256sum --check release-manifest.json.sha256',
      'gh attestation verify',
      '--signer-workflow',
      '--source-digest',
      '--source-ref refs/heads/main',
      'runDetails.metadata.invocationId',
    ]) {
      expect(promote).toContain(required);
    }
    expect(promote).toContain("ref: ${{ github.sha }}");
    expect(promote).not.toContain("pattern: release-candidate-*");
    expect(promote).not.toContain(":latest");
    expect(promote).toContain("registry-manifest-status.ts");
    expect(promote).toContain("BuildKit SPDX attestation is content-addressed to this exact");
    expect(promote).toContain("candidate SBOM does not match approved evidence");
    expect(promote).toContain("sbom.spdx.json.sha256");
    expect(candidate).toContain("registry-manifest-status.ts");
    expect(registryStatus).toContain("manifestResponse.status === 200 || manifestResponse.status === 404");
    expect(registryStatus).toContain("refusing to classify it as absent");
  });

  test("completes all immutable preflights before either external write", () => {
    const preflight = promote.indexOf("name: Preflight every release target");
    const imageWrite = promote.indexOf("docker buildx imagetools create --tag");
    const releaseWrite = promote.indexOf('gh release create "v$VERSION"');
    expect(preflight).toBeGreaterThan(0);
    expect(imageWrite).toBeGreaterThan(preflight);
    expect(releaseWrite).toBeGreaterThan(preflight);
    expect(promote.slice(0, preflight)).not.toContain("imagetools create --tag");
    expect(promote.slice(0, preflight)).not.toContain("gh release create");
  });

  test("marks promoted SemVer prereleases as GitHub prereleases", () => {
    expect(promote).toContain('if [[ "$VERSION" == *-* ]]');
    expect(promote).toContain("release_flags+=(--prerelease)");
    expect(promote).toContain('"${release_flags[@]}"');
  });
});

describe("container release inputs", () => {
  const dockerfile = readFileSync(join(root, "Dockerfile"), "utf8");
  const compose = readFileSync(join(root, "docker-compose.example.yml"), "utf8");

  test("pins the Docker frontend and Bun base and runs non-root", () => {
    expect(dockerfile).toMatch(/^# syntax=docker\/dockerfile:1\.7@sha256:[0-9a-f]{64}$/m);
    expect(dockerfile).toMatch(/oven\/bun:1\.4\.0-slim@sha256:e0ee68d16ccb9927bf02aa7dd8fd4bf3369ee6d46da04faa72b05ce8bfd135f6/);
    expect(dockerfile).toContain("USER bun");
    expect(dockerfile).toContain("org.opencontainers.image.base.digest");
    expect(dockerfile).toContain("org.rentemester.runtime.bun.version");
    expect(dockerfile).toContain("RENTEMESTER_BUN_VERSION");
    expect(dockerfile).toContain("RENTEMESTER_BASE_IMAGE_DIGEST");
    expect(dockerfile).toContain("RENTEMESTER_APP_AUTH=required");
    expect(dockerfile).toContain("COPY examples ./examples");
    expect(dockerfile).toContain("process.env.RENTEMESTER_APP_TOKEN");
    expect(dockerfile).toContain("Authorization:`Bearer ${token}`");
    expect(dockerfile).not.toContain(":latest");
  });

  test("smokes registered CLI examples from the built container", () => {
    expect(containerTest).toContain('"documents", "ingest", "--example"');
    expect(containerTest).toContain("documents ingest --example must emit valid metadata JSON");
  });

  test("keeps Docker's actual Bun base digest aligned with release runtime identity", () => {
    const imageDigest = dockerfile.match(/ARG BUN_IMAGE=oven\/bun:1\.4\.0-slim@(sha256:[0-9a-f]{64})/)?.[1];
    expect(imageDigest).toBeDefined();
    expect(dockerfile).toContain(`ARG RENTEMESTER_BASE_IMAGE_DIGEST=${imageDigest}`);
    expect(candidate).toContain(`BUN_BASE_IMAGE_DIGEST: ${imageDigest}`);
    expect(candidate).toContain("RENTEMESTER_BASE_IMAGE_DIGEST=${{ env.BUN_BASE_IMAGE_DIGEST }}");
  });

  test("requires a digest pin and keeps the no-login cockpit on host loopback", () => {
    expect(compose).toContain("RENTEMESTER_IMAGE:?");
    expect(compose).toContain('"127.0.0.1:4319:4319"');
    expect(compose).toContain("RENTEMESTER_DEPLOYMENT_PROFILE: local-container");
    expect(compose).toContain("RENTEMESTER_APP_AUTH: off");
    expect(dockerfile).toContain("RENTEMESTER_DEPLOYMENT_PROFILE=local-container");
    expect(compose).toContain("rentemester-workspace:/workspace");
    expect(compose).not.toContain("ghcr.io/mikkelkrogsholm/rentemester:v0.1.0");
  });
});
