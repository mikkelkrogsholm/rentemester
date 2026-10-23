import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { assertRustManifest, createRustManifest, type RustManifestMetadata } from "../../scripts/release/rust-manifest";

const hash = "a".repeat(64);
const pinsRaw = readFileSync(new URL("../../scripts/rust/pdf-worker/artifacts.json", import.meta.url), "utf8");
const pins = JSON.parse(pinsRaw);
const pdfLicenses = JSON.parse(readFileSync(new URL("../../scripts/rust/pdf-worker/license-inventory.json", import.meta.url), "utf8"));
const thirdPartyCredits = readFileSync(new URL("../../scripts/release/rust-third-party-notices.txt", import.meta.url), "utf8");
const sha = (raw: string) => createHash("sha256").update(raw).digest("hex");
const metadata: RustManifestMetadata = {
  version: "0.2.0", gitCommit: "b".repeat(40), builtAt: "2026-10-21T00:00:00Z",
  imageRepository: "ghcr.io/mikkelkrogsholm/rentemester", imageDigest: `sha256:${hash}`,
  workflowRunId: "12345", workflowRunAttempt: 1,
  sbomSha256: `sha256:${hash}`, supplyChainSha256: `sha256:${hash}`, agentDiscoverySha256: `sha256:${hash}`,
};
function fixture(target = "aarch64-unknown-linux-gnu") {
  const artifact = pins.artifacts.find((a: { archive: string }) => a.archive === (target.startsWith("aarch64") ? "pdfium-linux-arm64.tgz" : "pdfium-linux-x64.tgz"));
  const notices: { path: string; size: number; sha256: string }[] = pdfLicenses.artifacts.find((row: { archive: string }) => row.archive === artifact.archive).notices;
  const paths = ["THIRD-PARTY-NOTICES.txt", "bin/rentemester-server", "bin/rentemester-rust", "bin/rentemester-mcp", "bin/rentemester-pdf-worker", "bin/rustc.txt", "bin/cargo.txt", "native-server.json", "native-mcp-catalogue.json", "web/index.html", "debian-packages.txt", "bin/pdfium/artifact.json", `bin/pdfium/${artifact.library}`, ...notices.map(row => `bin/pdfium/${row.path}`)];
  return {
    format: "rentemester-rust-build/1",
    product: { ...metadataRelease(), sourceDateEpoch: Date.parse(metadata.builtAt) / 1000, sourceDirty: false },
    toolchain: { rustc: `rustc 1.94.1 (abcdef123 2026-09-01)\nbinary: rustc\ncommit-hash: ${"abcdef123" + "c".repeat(31)}\ncommit-date: 2026-09-01\nhost: ${target}\nrelease: 1.94.1\nLLVM version: 21.1.8`, cargo: "cargo 1.94.1 (abcdef123 2026-09-01)", target },
    baseImages: { rustBuilder: `rust:1.94.1@sha256:${hash}`, webBuilder: `oven/bun:1.4.0@sha256:${hash}`, runtime: `debian:trixie@sha256:${hash}` },
    schemas: { apiEpoch: 1, controlSchema: 31, companySchema: 36 },
    inputs: { cargoLockSha256: hash, webLockSha256: hash, pdfPinsSha256: sha(pinsRaw), entrypointSha256: hash, vendor: [{ path: "crate/Cargo.toml", size: 1, sha256: hash, executable: false }] },
    pdf: { artifact, binding: pins.binding, release: pins.release },
    files: paths.sort().map(path => {
      if (path === "THIRD-PARTY-NOTICES.txt") return { path, size: Buffer.byteLength(thirdPartyCredits), sha256: sha(thirdPartyCredits), executable: false };
      const notice = notices.find(row => path === `bin/pdfium/${row.path}`);
      return { path, size: notice?.size ?? 1, sha256: notice?.sha256 ?? (path.endsWith(artifact.library) ? artifact.librarySha256 : hash), executable: path.startsWith("bin/rentemester-") };
    }),
  };
}
function metadataRelease() { return { version: metadata.version, gitCommit: metadata.gitCommit, builtAt: metadata.builtAt }; }
function raw() { return JSON.stringify(fixture(), null, 2) + "\n"; }

describe("Rust release manifest v2", () => {
  test("nonempty but changed PDF license or notice cannot be promoted", () => {
    for (const path of ["bin/pdfium/LICENSE", "bin/pdfium/licenses/pdfium.txt"]) {
      const changed = fixture();
      const notice = changed.files.find(file => file.path === path)!;
      notice.sha256 = "f".repeat(64);
      expect(() => createRustManifest(JSON.stringify(changed), metadata)).toThrow("PDF license content mismatch");
    }
  });
  test("CLI emits v2 from the external raw identity and existing release environment", () => {
    const dir = mkdtempSync(join(tmpdir(), "rm-rust-manifest-"));
    try {
      const path = join(dir, "identity.json");
      writeFileSync(path, raw());
      const result = Bun.spawnSync([process.execPath, new URL("../../scripts/release/create-rust-manifest.ts", import.meta.url).pathname, path], {
        env: {
          ...process.env,
          RELEASE_VERSION: metadata.version, RELEASE_GIT_COMMIT: metadata.gitCommit,
          RELEASE_BUILT_AT: metadata.builtAt, RELEASE_IMAGE_REPOSITORY: metadata.imageRepository,
          RELEASE_IMAGE_DIGEST: metadata.imageDigest, RELEASE_WORKFLOW_RUN_ID: metadata.workflowRunId,
          RELEASE_WORKFLOW_RUN_ATTEMPT: String(metadata.workflowRunAttempt),
          RELEASE_SBOM_SHA256: metadata.sbomSha256, RELEASE_SUPPLY_CHAIN_SHA256: metadata.supplyChainSha256,
          RELEASE_AGENT_DISCOVERY_SHA256: metadata.agentDiscoverySha256,
        }, stdout: "pipe", stderr: "pipe",
      });
      expect(result.exitCode).toBe(0);
      expect(result.stderr.toString()).toBe("");
      expect(assertRustManifest(JSON.parse(result.stdout.toString()), raw())).toEqual(createRustManifest(raw(), metadata));
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
  test("approval CLI binds Rust identity, manifest bytes and human approval", () => {
    const dir = mkdtempSync(join(tmpdir(), "rm-rust-approval-"));
    try {
      const identityPath = join(dir, "identity.json");
      const manifestPath = join(dir, "manifest.json");
      const approvalPath = join(dir, "approval.json");
      const identity = raw();
      const manifestRaw = JSON.stringify(createRustManifest(identity, metadata)) + "\n";
      const approval = {
        schemaVersion: 1, approvalId: "synthetic-rust-approval",
        decision: "approved", approvedAt: "2026-10-21T01:00:00Z",
        reviewer: { organization: "Digisense", name: "Synthetic reviewer" },
        releaseManifestDigest: `sha256:${sha(manifestRaw)}`,
        imageDigest: metadata.imageDigest, version: metadata.version, gitCommit: metadata.gitCommit,
      };
      writeFileSync(identityPath, identity);
      writeFileSync(manifestPath, manifestRaw);
      writeFileSync(approvalPath, JSON.stringify(approval));
      const run = (includeIdentity = true) => Bun.spawnSync([
        process.execPath, new URL("../../scripts/release/verify-approval.ts", import.meta.url).pathname,
        manifestPath, approvalPath, ...(includeIdentity ? [identityPath] : []),
      ], { stdout: "pipe", stderr: "pipe" });
      expect(run().exitCode).toBe(0);
      const missing = run(false);
      expect(missing.exitCode).not.toBe(0);
      expect(missing.stderr.toString()).toContain("exact packaged build-identity.json");
      writeFileSync(identityPath, identity + " ");
      expect(run().exitCode).not.toBe(0);
      writeFileSync(identityPath, identity);
      writeFileSync(approvalPath, JSON.stringify({ ...approval, imageDigest: `sha256:${"f".repeat(64)}` }));
      expect(run().exitCode).not.toBe(0);
      writeFileSync(approvalPath, JSON.stringify(approval));
      writeFileSync(manifestPath, manifestRaw + " ");
      const changedManifest = run();
      expect(changedManifest.exitCode).not.toBe(0);
      expect(changedManifest.stderr.toString()).toContain("releaseManifestDigest does not match");
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
  test("binds clean actual-shape identity, platform and exact raw bytes", () => {
    const identity = raw();
    const manifest = createRustManifest(identity, metadata);
    expect(manifest.manifestVersion).toBe(2);
    expect(manifest.image.platforms).toEqual(["linux/arm64"]);
    expect(manifest.runtime).toEqual({ kind: "rust", buildIdentitySha256: `sha256:${sha(identity)}` });
    expect(assertRustManifest(manifest, identity)).toEqual(manifest);
    expect(createRustManifest(JSON.stringify(fixture("x86_64-unknown-linux-gnu")), metadata).image.platforms).toEqual(["linux/amd64"]);
  });
  test("provenance equality ignores object key ordering, not values", () => {
    const identity = raw();
    const manifest = createRustManifest(identity, metadata);
    manifest.provenance = Object.fromEntries(Object.entries(manifest.provenance).reverse());
    expect(assertRustManifest(manifest, identity)).toEqual(manifest);
    expect(() => assertRustManifest(manifest, identity + " ")).toThrow("hash mismatch");
    (manifest.provenance.schemas as Record<string, unknown>).companySchema = 999;
    expect(() => assertRustManifest(manifest, identity)).toThrow("provenance differs");
  });
  test.each(["version", "gitCommit", "builtAt"] as const)("refuses wrong release %s", key => {
    const wrong = { ...metadata, [key]: key === "version" ? "0.2.1" : key === "gitCommit" ? "d".repeat(40) : "2026-10-22T00:00:00Z" };
    expect(() => createRustManifest(raw(), wrong)).toThrow("mismatch");
  });
  test("dirty source, time drift, schemas, target and compiler are refused", () => {
    const dirty = fixture(); dirty.product.sourceDirty = true;
    expect(() => createRustManifest(JSON.stringify(dirty), metadata)).toThrow("dirty source");
    const time = fixture(); time.product.sourceDateEpoch++;
    expect(() => createRustManifest(JSON.stringify(time), metadata)).toThrow("epoch");
    const target = fixture(); target.toolchain.target = "aarch64-apple-darwin";
    expect(() => createRustManifest(JSON.stringify(target), metadata)).toThrow("target");
    const compiler = fixture(); compiler.toolchain.rustc = "rustc 1.94.1";
    expect(() => createRustManifest(JSON.stringify(compiler), metadata)).toThrow("rustc");
    const cargo = fixture(); cargo.toolchain.cargo = "cargo unknown";
    expect(() => createRustManifest(JSON.stringify(cargo), metadata)).toThrow("Cargo");
    const schema = fixture(); schema.schemas.apiEpoch = 0;
    expect(() => createRustManifest(JSON.stringify(schema), metadata)).toThrow("positive integer");
  });
  test("unsafe, duplicate, unsorted and missing required inventory refuses", () => {
    for (const path of ["../escape", "/absolute", "a/../escape", "a\\escape", "a//escape"]) {
      const bad = fixture(); bad.files[0]!.path = path;
      expect(() => createRustManifest(JSON.stringify(bad), metadata)).toThrow();
    }
    const duplicate = fixture(); duplicate.files.splice(1, 0, duplicate.files[0]!);
    expect(() => createRustManifest(JSON.stringify(duplicate), metadata)).toThrow("sorted and unique");
    const unsorted = fixture(); unsorted.files.reverse();
    expect(() => createRustManifest(JSON.stringify(unsorted), metadata)).toThrow("sorted and unique");
    for (const required of ["bin/rentemester-server", "bin/rentemester-rust", "bin/rentemester-mcp", "bin/rentemester-pdf-worker", "native-server.json", "native-mcp-catalogue.json", "debian-packages.txt", "web/index.html"]) {
      const bad = fixture(); bad.files = bad.files.filter(f => f.path !== required);
      expect(() => createRustManifest(JSON.stringify(bad), metadata)).toThrow("missing runtime file");
    }
  });
  test("PDF source pins, library and digest inputs are bound", () => {
    const bad = fixture(); bad.inputs.pdfPinsSha256 = "f".repeat(64);
    expect(() => createRustManifest(JSON.stringify(bad), metadata)).toThrow("PDF source pins");
    const artifact = fixture(); artifact.pdf.artifact = { ...artifact.pdf.artifact, sha256: hash };
    expect(() => createRustManifest(JSON.stringify(artifact), metadata)).toThrow("PDF source artifact");
    const library = fixture(); library.files.find(f => f.path.endsWith("libpdfium.so"))!.sha256 = hash;
    expect(() => createRustManifest(JSON.stringify(library), metadata)).toThrow("library hash");
    const image = fixture(); image.baseImages.runtime = "debian:latest";
    expect(() => createRustManifest(JSON.stringify(image), metadata)).toThrow("runtime");
  });
  test("mixed Bun and unknown fields fail closed", () => {
    const identity = fixture() as unknown as Record<string, unknown>;
    identity.bunVersion = "1.4.0";
    expect(() => createRustManifest(JSON.stringify(identity), metadata)).toThrow("fields");
    const manifest = createRustManifest(raw(), metadata);
    Object.assign(manifest.runtime, { bunVersion: "1.4.0" });
    expect(() => assertRustManifest(manifest, raw())).toThrow("fields");
  });
  test("manifest repository, source, platform, hash and workflow bindings refuse", () => {
    const mutations = [
      (m: ReturnType<typeof createRustManifest>) => { m.image.repository = "ghcr.io/other/image"; },
      (m: ReturnType<typeof createRustManifest>) => { m.image.platforms = ["linux/amd64"]; },
      (m: ReturnType<typeof createRustManifest>) => { m.source.ref = "c".repeat(40); },
      (m: ReturnType<typeof createRustManifest>) => { m.workflow.runId = "no"; },
      (m: ReturnType<typeof createRustManifest>) => { m.workflow.runAttempt = 0; },
      (m: ReturnType<typeof createRustManifest>) => { m.runtime.buildIdentitySha256 = `sha256:${"f".repeat(64)}`; },
    ];
    for (const mutate of mutations) {
      const manifest = createRustManifest(raw(), metadata); mutate(manifest);
      expect(() => assertRustManifest(manifest, raw())).toThrow();
    }
    for (const key of ["sbomSha256", "supplyChainSha256", "agentDiscoverySha256"] as const) {
      expect(() => createRustManifest(raw(), { ...metadata, [key]: hash })).toThrow(key);
    }
  });
});
