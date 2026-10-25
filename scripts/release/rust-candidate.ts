#!/usr/bin/env bun
/** Existing candidate workflow's Rust branch. No registry writes occur here. */
import { createHash } from "node:crypto";
import { existsSync, lstatSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { isValidSemVer } from "../../src/core/semver";

export const PLATFORM = "linux/amd64";
const BUILDKIT = "moby/buildkit:buildx-stable-1@sha256:28a898719c18a33f4e8000685287fa36fd0dd9560c6440227d3a732d79bb41d8";
export type Identity = { version: string; commit: string; epoch: string; builtAt: string; run: string; attempt: string };
export type Run = (argv: string[], env?: NodeJS.ProcessEnv) => Buffer;
export function identity(env: NodeJS.ProcessEnv): Identity {
  const required = (key: string) => { const value = env[key]; if (!value || value.trim() !== value) throw new Error(`${key} is required`); return value; };
  const version = required("RELEASE_VERSION");
  const commit = required("RELEASE_GIT_COMMIT");
  const epoch = required("SOURCE_DATE_EPOCH");
  const run = required("GITHUB_RUN_ID");
  const attempt = required("GITHUB_RUN_ATTEMPT");
  if (!isValidSemVer(version) || version.includes("+") || !/^[0-9a-f]{40}$/.test(commit)
    || ![epoch, run, attempt].every(v => /^[1-9][0-9]*$/.test(v))) throw new Error("invalid Rust candidate identity");
  const builtAt = new Date(Number(epoch) * 1000).toISOString().replace(".000Z", "Z");
  if (env.RELEASE_BUILT_AT !== builtAt) throw new Error("source epoch and build time differ");
  return { version, commit, epoch, builtAt, run, attempt };
}
export function immutableImage(repository: string | undefined, digest: string | undefined): string {
  if (repository !== "ghcr.io/mikkelkrogsholm/rentemester" || !digest || !/^sha256:[0-9a-f]{64}$/.test(digest)) throw new Error("exact immutable candidate digest required");
  return `${repository}@${digest}`;
}
export function buildArgs(i: Identity, variant: string): string[] {
  if (!["first", "second", "readiness", "publisher"].includes(variant)) throw new Error("unknown independent build variant");
  return ["RENTEMESTER_VERSION=" + i.version, "RENTEMESTER_GIT_COMMIT=" + i.commit,
    "RENTEMESTER_BUILT_AT=" + i.builtAt, "SOURCE_DATE_EPOCH=" + i.epoch,
    "RENTEMESTER_SOURCE_DIRTY=false", "CARGO_BUILD_JOBS=2", `RUST_BUILD_CACHE_ID=release-${i.run}-${i.attempt}-${variant}`]
    .flatMap(arg => ["--build-arg", arg]);
}
export function buildCommand(i: Identity, variant: string, builder: string, output: string): string[] {
  return ["docker", "buildx", "build", "--builder", builder, "--file", "Dockerfile.rust", "--platform", PLATFORM,
    "--no-cache", "--provenance=false", "--sbom=false", ...buildArgs(i, variant), "--output", output, "."];
}
function sha(bytes: Uint8Array): string { return createHash("sha256").update(bytes).digest("hex"); }
function newFile(path: string, bytes: Uint8Array | string): void { writeFileSync(path, bytes, { flag: "wx", mode: 0o600 }); }
export function checksum(path: string): string {
  const digest = sha(readFileSync(path));
  const sidecar=`${path}.sha256`;
  const expected=`${digest}  ${basename(path)}\n`;
  if(existsSync(sidecar)){
    if(!lstatSync(sidecar).isFile() || readFileSync(sidecar,"utf8")!==expected) throw new Error("existing evidence checksum does not match exact bytes and basename");
  }else{newFile(sidecar,expected);}
  return `sha256:${digest}`;
}
function run(argv: string[], env: NodeJS.ProcessEnv = process.env): Buffer {
  const p = Bun.spawnSync(argv, { env, stdout: "pipe", stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`${argv[0]} ${argv[1]} failed (${p.exitCode}): ${p.stderr.toString()}`);
  return Buffer.from(p.stdout);
}
export async function reproducibility(i: Identity, execute: Run = run): Promise<void> {
  const directory = mkdtempSync(join(tmpdir(), "rentemester-rust-repro-"));
  const archives: string[] = [];
  try {
    for (const variant of ["first", "second"]) {
      const builder = `rm-rust-${i.run}-${i.attempt}-${variant}`;
      const archive = join(directory, `${variant}.oci.tar`);
      let created = false;
      try {
        execute(["docker", "buildx", "create", "--name", builder, "--driver", "docker-container", "--driver-opt", `image=${BUILDKIT}`]); created = true;
        execute(["docker", "buildx", "inspect", "--builder", builder, "--bootstrap"]);
        execute(buildCommand(i, variant, builder, `type=oci,dest=${archive},rewrite-timestamp=true`));
        archives.push(archive);
      } finally { if (created && variant === "second") execute(["docker", "buildx", "rm", builder]); }
    }
    const hashes: string[] = [];
    for (const archive of archives) {
      const hash = createHash("sha256");
      for await (const chunk of Bun.file(archive).stream()) hash.update(chunk);
      hashes.push(hash.digest("hex"));
    }
    const digests = archives.map(a => JSON.parse(execute(["tar", "-xOf", a, "index.json"]).toString()).manifests?.[0]?.digest);
    if (!/^sha256:[0-9a-f]{64}$/.test(digests[0]) || hashes[0] !== hashes[1] || digests[0] !== digests[1]) throw new Error("independent cold Rust OCI exports differ");
    const manifest = JSON.parse(execute(["tar", "-xOf", archives[0]!, `blobs/sha256/${digests[0].slice(7)}`]).toString());
    if (!/^sha256:[0-9a-f]{64}$/.test(manifest.config?.digest)) throw new Error("cold OCI image config missing");
    const config = JSON.parse(execute(["tar", "-xOf", archives[0]!, `blobs/sha256/${manifest.config.digest.slice(7)}`]).toString());
    if (config.os !== "linux" || config.architecture !== "amd64") throw new Error("cold OCI platform differs from release platform");
    const report = { status: "passed", platform: PLATFORM, digest: digests[0], configDigest: manifest.config.digest, archiveSha256: hashes[0] };
    newFile("rust-reproducibility.json", `${JSON.stringify(report)}\n`);
    console.log(JSON.stringify(report));
  } finally { rmSync(directory, { recursive: true, force: true }); }
}
export function sbomTemplate(raw: string): string {
  const value = JSON.parse(raw);
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("BuildKit SBOM missing");
  if (Object.hasOwn(value, PLATFORM)) return '{{ json (index .SBOM "linux/amd64").SPDX }}';
  if (value.SPDX && Object.keys(value).every(k => ["SPDX", "CycloneDX"].includes(k))) return "{{ json .SBOM.SPDX }}";
  throw new Error("BuildKit SBOM has no unambiguous linux/amd64 SPDX document");
}
export function qualifierCommand(image: string, i: Identity): string[] {
  return ["python3", "scripts/release/qualify-rust-candidate.py", "--source", ".", "--image", image,
    "--identity", "rust-build-identity.json", "--package-evidence", "rust-package-evidence.json",
    "--license-evidence", "rust-license-evidence.json", "--sbom", "sbom.spdx.json",
    "--commit", i.commit, "--version", i.version, "--platform", PLATFORM,
    "--output", "supply-chain-evidence.json", "--discovery-output", "agent-discovery-coverage-report.json"];
}
export function qualify(i: Identity, image: string, execute: Run = run): void {
  execute(["docker", "pull", image]);
  execute(["python3", "scripts/release/verify-rust-package.py", "--image", image,
    "--identity", "rust-build-identity.json", "--evidence", "rust-package-evidence.json", "--commit", i.commit, "--version", i.version, "--require-clean"]);
  const directory = mkdtempSync(join(tmpdir(), "rentemester-rust-licenses-"));
  try {
    const path = join(directory, "licenses.json");
    execute(["python3", "scripts/release/verify-rust-licenses.py", "--output", path]);
    newFile("rust-license-evidence.json", readFileSync(path));
  } finally { rmSync(directory, { recursive: true, force: true }); }
  const template = sbomTemplate(execute(["docker", "buildx", "imagetools", "inspect", image, "--format", "{{ json .SBOM }}"]).toString());
  const raw = execute(["docker", "buildx", "imagetools", "inspect", image, "--format", template]);
  const spdx = JSON.parse(raw.toString());
  if (!spdx || typeof spdx !== "object" || Array.isArray(spdx) || !/^SPDX-2\./.test(spdx.spdxVersion) || !Array.isArray(spdx.packages) || !spdx.packages.length) throw new Error("actual BuildKit SPDX is incomplete");
  newFile("sbom.spdx.json", raw); // Preserve fetched SPDX bytes for independent approval.
  execute(qualifierCommand(image, i)); // Full original discovery parity must pass; never fall back to Bun.
  const hashes = Object.fromEntries(["rust-build-identity.json", "rust-package-evidence.json", "rust-license-evidence.json", "sbom.spdx.json", "supply-chain-evidence.json", "agent-discovery-coverage-report.json"].map(path => [path, checksum(path)]));
  const env = { ...process.env, RELEASE_VERSION: i.version, RELEASE_GIT_COMMIT: i.commit,
    RELEASE_BUILT_AT: i.builtAt, RELEASE_IMAGE_REPOSITORY: image.split("@")[0], RELEASE_IMAGE_DIGEST: image.split("@")[1],
    RELEASE_WORKFLOW_RUN_ID: i.run, RELEASE_WORKFLOW_RUN_ATTEMPT: i.attempt,
    RELEASE_SBOM_SHA256: hashes["sbom.spdx.json"], RELEASE_SUPPLY_CHAIN_SHA256: hashes["supply-chain-evidence.json"], RELEASE_AGENT_DISCOVERY_SHA256: hashes["agent-discovery-coverage-report.json"] };
  newFile("release-manifest.json", execute(["bun", "run", "scripts/release/create-rust-manifest.ts", "rust-build-identity.json"], env));
  checksum("release-manifest.json");
}
export function assertReproducedImage(proof: {status?:unknown;platform?:unknown;configDigest?:unknown}, actual: {Id?:unknown;Os?:unknown;Architecture?:unknown}):void {
  if (proof.status!=="passed" || proof.platform!==PLATFORM || typeof proof.configDigest!=="string" || !/^sha256:[0-9a-f]{64}$/.test(proof.configDigest)
    || actual.Id!==proof.configDigest || actual.Os!=="linux" || actual.Architecture!=="amd64") throw new Error("published image differs from independently reproduced linux/amd64 bytes");
}
export function publishedPlatformManifest(raw: {manifests?:Array<{digest?:string;platform?:{os?:string;architecture?:string};annotations?:Record<string,string>}>;config?:unknown}, rootDigest:string):string {
  if (raw.config && !raw.manifests) return rootDigest;
  if (!Array.isArray(raw.manifests)) throw new Error("published OCI index is invalid");
  const images=raw.manifests.filter(d=>d.platform?.os==="linux"&&d.platform.architecture==="amd64");
  if (images.length!==1 || raw.manifests.some(d=>!images.includes(d)&&d.annotations?.["vnd.docker.reference.type"]!=="attestation-manifest")) throw new Error("published OCI index is not exactly one linux/amd64 image plus attestations");
  const digest=images[0]!.digest;
  if (!digest || !/^sha256:[0-9a-f]{64}$/.test(digest)) throw new Error("published platform manifest digest missing");
  return digest;
}
function reproduced(image:string, published=false):void {
  const proof=JSON.parse(readFileSync("rust-reproducibility.json","utf8"));
  assertReproducedImage(proof, JSON.parse(run(["docker","image","inspect",image]).toString())[0]);
  if(published){
    const raw=JSON.parse(run(["docker","buildx","imagetools","inspect",image,"--raw"]).toString());
    if(publishedPlatformManifest(raw,image.split("@")[1]!)!==proof.digest) throw new Error("published platform manifest differs from independently reproduced OCI bytes");
  }
}
export async function main(stage: string, env: NodeJS.ProcessEnv = process.env): Promise<void> {
  const i = identity(env);
  if (stage === "reproducibility") return reproducibility(i);
  if (stage === "readiness") {
    const image = `rentemester-rust-preflight:${i.run}-${i.attempt}`;
    run(["docker", "buildx", "build", "--builder", `rm-rust-${i.run}-${i.attempt}-first`, "--file", "Dockerfile.rust", "--platform", PLATFORM, ...buildArgs(i, "first"), "--load", "--tag", image, "."]);
    reproduced(image);
    try { run(["python3", "scripts/rust/run-rust-container.py", "--image", image, "--evidence", "rust-container-preflight.json"]); }
    finally { run(["docker", "image", "rm", image]); }
    return;
  }
  const image = immutableImage(env.REGISTRY_IMAGE, env.IMAGE_DIGEST);
  if (stage === "smoke") { run(["docker", "pull", image]); reproduced(image, true); return; }
  if (stage === "qualify") { qualify(i, image); return; }
  throw new Error("usage: rust-candidate.ts reproducibility|readiness|smoke|qualify");
}
if (import.meta.main) await main(process.argv[2] ?? "");
