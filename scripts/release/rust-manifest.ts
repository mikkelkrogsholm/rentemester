import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";
import { isValidSemVer } from "../../src/core/semver";

const SOURCE = "https://github.com/mikkelkrogsholm/rentemester";
const IMAGE = "ghcr.io/mikkelkrogsholm/rentemester";
const pinsRaw = readFileSync(new URL("../rust/pdf-worker/artifacts.json", import.meta.url), "utf8");
const pins = JSON.parse(pinsRaw);
const pdfLicensesRaw = readFileSync(new URL("../rust/pdf-worker/license-inventory.json", import.meta.url), "utf8");
const pdfLicenses = JSON.parse(pdfLicensesRaw);
const licensePolicy = JSON.parse(readFileSync(new URL("./pdfium-license-approval.json", import.meta.url), "utf8"));
const thirdPartyCredits = readFileSync(new URL("./rust-third-party-notices.txt", import.meta.url), "utf8");
type ObjectValue = Record<string, unknown>;
export type RustManifestMetadata = {
  version: string; gitCommit: string; builtAt: string;
  imageRepository: string; imageDigest: string;
  workflowRunId: string; workflowRunAttempt: number;
  sbomSha256: string; supplyChainSha256: string; agentDiscoverySha256: string;
};
export type RustManifest = {
  manifestVersion: 2;
  release: { version: string; gitCommit: string; builtAt: string };
  source: { repository: string; ref: string };
  workflow: { runId: string; runAttempt: number };
  image: { repository: string; digest: string; platforms: string[] };
  runtime: { kind: "rust"; buildIdentitySha256: string };
  evidence: { sbomSha256: string; supplyChainSha256: string; agentDiscoverySha256: string };
  provenance: ObjectValue;
};
function requireValue(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`Rust release manifest: ${message}`);
}
function object(value: unknown, keys: string[], label: string): ObjectValue {
  requireValue(value !== null && typeof value === "object" && !Array.isArray(value), `${label} must be an object`);
  const actual = Object.keys(value).sort();
  requireValue(isDeepStrictEqual(actual, [...keys].sort()), `${label} fields are invalid`);
  return value as ObjectValue;
}
function text(value: unknown, pattern: RegExp, label: string): asserts value is string {
  requireValue(typeof value === "string" && pattern.test(value), `${label} is invalid`);
}
function positive(value: unknown, label: string): asserts value is number {
  requireValue(Number.isSafeInteger(value) && Number(value) > 0, `${label} must be a positive integer`);
}
const HEX = /^[0-9a-f]{64}$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
function sha(raw: string): string { return createHash("sha256").update(raw, "utf8").digest("hex"); }
function safePath(value: unknown): asserts value is string {
  text(value, /^[A-Za-z0-9_.+@-]+(?:\/[A-Za-z0-9_.+@-]+)*$/, "inventory path");
  requireValue(value.split("/").every(p => p !== "." && p !== ".."), "inventory traversal");
}
function inventory(value: unknown, label: string): ObjectValue[] {
  requireValue(Array.isArray(value) && value.length > 0, `${label} inventory is empty`);
  let previous = "";
  return value.map(row => {
    const file = object(row, ["path", "size", "sha256", "executable"], label);
    safePath(file.path);
    requireValue(file.path > previous, `${label} inventory must be sorted and unique`);
    previous = file.path;
    requireValue(Number.isSafeInteger(file.size) && Number(file.size) >= 0, `${label} size is invalid`);
    text(file.sha256, HEX, `${label} SHA256`);
    requireValue(typeof file.executable === "boolean", `${label} executable is invalid`);
    return file;
  });
}
function identity(raw: string): { value: ObjectValue; platform: string } {
  requireValue(typeof raw === "string" && Buffer.byteLength(raw) <= 32 * 1024 * 1024, "identity must be bounded raw UTF-8");
  const value = object(JSON.parse(raw), ["format", "product", "toolchain", "baseImages", "schemas", "inputs", "pdf", "files"], "identity");
  requireValue(value.format === "rentemester-rust-build/1", "identity format");
  const product = object(value.product, ["version", "gitCommit", "builtAt", "sourceDateEpoch", "sourceDirty"], "product");
  requireValue(typeof product.version === "string" && isValidSemVer(product.version) && !product.version.includes("+"), "release-safe product version");
  text(product.gitCommit, /^[0-9a-f]{40}$/, "source commit");
  positive(product.sourceDateEpoch, "source epoch");
  requireValue(product.sourceDirty === false, "dirty source cannot be promoted");
  requireValue(typeof product.builtAt === "string" && new Date(product.sourceDateEpoch * 1000).toISOString().replace(".000Z", "Z") === product.builtAt, "build time does not match source epoch");
  const toolchain = object(value.toolchain, ["rustc", "cargo", "target"], "toolchain");
  const targets: Record<string, string> = { "aarch64-unknown-linux-gnu": "linux/arm64", "x86_64-unknown-linux-gnu": "linux/amd64" };
  requireValue(typeof toolchain.target === "string" && Object.hasOwn(targets, toolchain.target), "unsupported Linux target");
  text(toolchain.rustc, /^rustc (\d+\.\d+\.\d+) \([0-9a-f]{7,40} \d{4}-\d{2}-\d{2}\)\nbinary: rustc\ncommit-hash: [0-9a-f]{40}\ncommit-date: \d{4}-\d{2}-\d{2}\nhost: [a-z0-9_-]+\nrelease: \d+\.\d+\.\d+\nLLVM version: \d+\.\d+\.\d+$/, "raw rustc identity");
  const compiler = toolchain.rustc.split("\n");
  const header = /^rustc \S+ \(([0-9a-f]+) (\d{4}-\d{2}-\d{2})\)$/.exec(compiler[0]!)!;
  requireValue(compiler[2]!.slice("commit-hash: ".length).startsWith(header[1]!) && compiler[3] === `commit-date: ${header[2]}`, "compiler commit/date mismatch");
  requireValue(compiler[4] === `host: ${toolchain.target}` && compiler[5] === `release: ${compiler[0]!.split(" ")[1]}`, "compiler target/release mismatch");
  text(toolchain.cargo, /^cargo \d+\.\d+\.\d+ \([0-9a-f]{7,40} \d{4}-\d{2}-\d{2}\)$/, "raw Cargo identity");
  const images = object(value.baseImages, ["rustBuilder", "webBuilder", "runtime"], "base images");
  for (const [key, image] of Object.entries(images)) text(image, /^[^\s@]+@sha256:[0-9a-f]{64}$/, key);
  const schemas = object(value.schemas, ["apiEpoch", "controlSchema", "companySchema"], "schemas");
  for (const [key, number] of Object.entries(schemas)) positive(number, key);
  const inputs = object(value.inputs, ["cargoLockSha256", "webLockSha256", "pdfPinsSha256", "vendor", "entrypointSha256"], "inputs");
  for (const key of ["cargoLockSha256", "webLockSha256", "pdfPinsSha256", "entrypointSha256"]) text(inputs[key], HEX, key);
  requireValue(inputs.pdfPinsSha256 === sha(pinsRaw), "PDF source pins hash mismatch");
  inventory(inputs.vendor, "vendor");
  const pdf = object(value.pdf, ["artifact", "binding", "release"], "PDF");
  const archive = toolchain.target === "aarch64-unknown-linux-gnu" ? "pdfium-linux-arm64.tgz" : "pdfium-linux-x64.tgz";
  const artifact = pins.artifacts.find((a: { archive: string }) => a.archive === archive);
  requireValue(isDeepStrictEqual(pdf.artifact, artifact) && isDeepStrictEqual(pdf.binding, pins.binding) && pdf.release === pins.release, "PDF source artifact/binding pins mismatch");
  const files = inventory(value.files, "runtime");
  const byPath = new Map(files.map(file => [file.path, file]));
  const credits = byPath.get("THIRD-PARTY-NOTICES.txt");
  requireValue(credits && credits.sha256 === sha(thirdPartyCredits) &&
    credits.size === Buffer.byteLength(thirdPartyCredits), "required third-party credits mismatch");
  for (const path of ["bin/rentemester-server", "bin/rentemester-rust", "bin/rentemester-mcp", "bin/rentemester-pdf-worker", "bin/rustc.txt", "bin/cargo.txt", "native-server.json", "native-mcp-catalogue.json", "web/index.html", "debian-packages.txt", "bin/pdfium/artifact.json", "bin/pdfium/LICENSE", `bin/pdfium/${artifact.library}`]) {
    const file = byPath.get(path);
    requireValue(file && Number(file.size) > 0, `missing runtime file ${path}`);
    if (path.startsWith("bin/rentemester-")) requireValue(file.executable === true, `runtime executable mode ${path}`);
  }
  requireValue(byPath.get(`bin/pdfium/${artifact.library}`)!.sha256 === artifact.librarySha256, "PDF library hash mismatch");
  requireValue(files.some(file => String(file.path).startsWith("bin/pdfium/licenses/") && Number(file.size) > 0), "PDF third-party licenses missing");
  requireValue(licensePolicy.format === "rentemester-pdfium-license-approval/1" &&
    licensePolicy.dependencyApproved === true && pdfLicenses.format === "rentemester-pdfium-license-inventory/1" &&
    pdfLicenses.policyApprovalClaim === false && licensePolicy.inventorySha256 === sha(pdfLicensesRaw) &&
    licensePolicy.pinsSha256 === sha(pinsRaw) && pdfLicenses.pinsSha256 === sha(pinsRaw),
  "PDF licenses lack exact reviewed inventory/policy binding");
  const licenses = pdfLicenses.artifacts.filter((row: { archive: string }) => row.archive === archive);
  requireValue(licenses.length === 1 && licenses[0].archiveSha256 === artifact.sha256 &&
    licenses[0].library === artifact.library && licenses[0].librarySha256 === artifact.librarySha256,
  "PDF license artifact mismatch");
  const expectedNotices = licenses[0].notices.map((row: { path: string; size: number; sha256: string }) => {
    safePath(row.path);
    requireValue(row.path === "LICENSE" || row.path.startsWith("licenses/"), "invalid PDF notice path");
    const path = `bin/pdfium/${row.path}`;
    const file = byPath.get(path);
    requireValue(file && file.size === row.size && file.sha256 === row.sha256,
      `PDF license content mismatch ${row.path}`);
    return path;
  }).sort();
  const actualNotices = files.filter(file => file.path === "bin/pdfium/LICENSE" ||
    String(file.path).startsWith("bin/pdfium/licenses/")).map(file => file.path).sort();
  requireValue(expectedNotices.length > 1 && new Set(expectedNotices).size === expectedNotices.length &&
    isDeepStrictEqual(actualNotices, expectedNotices), "PDF complete license notice set mismatch");
  requireValue(!byPath.has("build-identity.json"), "self-referential inventory");
  return { value, platform: targets[toolchain.target]! };
}
export function createRustManifest(identityRaw: string, metadata: RustManifestMetadata): RustManifest {
  const parsed = identity(identityRaw);
  const manifest: RustManifest = {
    manifestVersion: 2,
    release: { version: metadata.version, gitCommit: metadata.gitCommit, builtAt: metadata.builtAt },
    source: { repository: SOURCE, ref: metadata.gitCommit },
    workflow: { runId: metadata.workflowRunId, runAttempt: metadata.workflowRunAttempt },
    image: { repository: metadata.imageRepository, digest: metadata.imageDigest, platforms: [parsed.platform] },
    runtime: { kind: "rust", buildIdentitySha256: `sha256:${sha(identityRaw)}` },
    evidence: { sbomSha256: metadata.sbomSha256, supplyChainSha256: metadata.supplyChainSha256, agentDiscoverySha256: metadata.agentDiscoverySha256 },
    provenance: parsed.value,
  };
  return assertRustManifest(manifest, identityRaw);
}
export function assertRustManifest(value: unknown, identityRaw: string): RustManifest {
  const parsed = identity(identityRaw);
  const manifest = object(value, ["manifestVersion", "release", "source", "workflow", "image", "runtime", "evidence", "provenance"], "manifest");
  requireValue(manifest.manifestVersion === 2, "manifest version must be 2");
  const release = object(manifest.release, ["version", "gitCommit", "builtAt"], "release");
  const product = parsed.value.product as ObjectValue;
  for (const key of ["version", "gitCommit", "builtAt"]) requireValue(release[key] === product[key], `release ${key} mismatch`);
  const source = object(manifest.source, ["repository", "ref"], "source");
  requireValue(source.repository === SOURCE && source.ref === product.gitCommit, "source binding mismatch");
  const workflow = object(manifest.workflow, ["runId", "runAttempt"], "workflow");
  text(workflow.runId, /^\d+$/, "workflow run ID");
  positive(workflow.runAttempt, "workflow run attempt");
  const image = object(manifest.image, ["repository", "digest", "platforms"], "image");
  requireValue(image.repository === IMAGE && isDeepStrictEqual(image.platforms, [parsed.platform]), "image repository/platform mismatch");
  text(image.digest, DIGEST, "image digest");
  const runtime = object(manifest.runtime, ["kind", "buildIdentitySha256"], "runtime");
  requireValue(runtime.kind === "rust" && runtime.buildIdentitySha256 === `sha256:${sha(identityRaw)}`, "exact packaged identity hash mismatch");
  requireValue(isDeepStrictEqual(manifest.provenance, parsed.value), "provenance differs from packaged identity");
  const evidence = object(manifest.evidence, ["sbomSha256", "supplyChainSha256", "agentDiscoverySha256"], "evidence");
  for (const [key, hash] of Object.entries(evidence)) text(hash, DIGEST, key);
  return manifest as unknown as RustManifest;
}
