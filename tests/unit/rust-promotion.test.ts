import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { lstatSync, readFileSync, mkdtempSync, writeFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createRustManifest, type RustManifestMetadata } from "../../scripts/release/rust-manifest";
import { promotionRuntime, verifyPromotionArtifacts, RUST_PROMOTION_FILES } from "../../scripts/release/rust-promotion";

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


const expected = {commit: metadata.gitCommit, runId: metadata.workflowRunId, runAttempt: "1"};
function candidate(runtime: "rust" | "bun" = "rust") {
  const dir = mkdtempSync(join(tmpdir(), "rm-rust-promotion-"));
  const files: Record<string,string> = {
    "sbom.spdx.json": "{\"spdxVersion\":\"SPDX-2.3\"}\n",
    "supply-chain-evidence.json": "{\"synthetic\":true}\n",
    "agent-discovery-coverage-report.json": "{\"synthetic\":true}\n",
  };
  const meta = {...metadata, sbomSha256:`sha256:${sha(files["sbom.spdx.json"]!)}`, supplyChainSha256:`sha256:${sha(files["supply-chain-evidence.json"]!)}`, agentDiscoverySha256:`sha256:${sha(files["agent-discovery-coverage-report.json"]!)}`};
  const manifest = runtime === "rust" ? createRustManifest(raw(), meta) : {
    manifestVersion:1, release:{version:meta.version,gitCommit:meta.gitCommit,builtAt:meta.builtAt},
    workflow:{runId:meta.workflowRunId,runAttempt:meta.workflowRunAttempt}, image:{repository:meta.imageRepository,digest:meta.imageDigest},
    runtime:{bunVersion:"1.4.0",baseImageDigest:`sha256:${hash}`},
    evidence:{sbomSha256:meta.sbomSha256,supplyChainSha256:meta.supplyChainSha256,agentDiscoverySha256:meta.agentDiscoverySha256},
  };
  files["release-manifest.json"] = JSON.stringify(manifest) + "\n";
  if (runtime === "rust") Object.assign(files,{"rust-build-identity.json":raw(),"rust-package-evidence.json":"{\"status\":\"passed\"}\n","rust-license-evidence.json":"{\"synthetic\":true}\n"});
  for (const [name,value] of Object.entries(files)) save(dir,name,value);
  return {dir, manifest};
}
function save(dir:string,name:string,value:string) {
  writeFileSync(join(dir,name),value);
  writeFileSync(join(dir,name+".sha256"),`${sha(value)}  ${name}\n`);
}
function using(run:(x:ReturnType<typeof candidate>)=>void,runtime:"rust"|"bun"="rust") {
  const x=candidate(runtime);try {run(x);}finally {rmSync(x.dir,{recursive:true,force:true});}
}
describe("Rust promotion of approved original artifacts",()=>{
  test("accepts exact raw v2 identity and all original evidence checksums",()=>using(({dir})=>{
    expect(verifyPromotionArtifacts(dir,expected)).toEqual({runtime:"rust",manifestVersion:2});
    expect(RUST_PROMOTION_FILES.every(n=>lstatSync(join(dir,n)).size>0)).toBe(true);
  }));
  test("preserves explicit v1 without a Rust identity or fallback",()=>using(({dir,manifest})=>{
    expect(verifyPromotionArtifacts(dir,expected)).toEqual({runtime:"bun",manifestVersion:1});
    expect(()=>promotionRuntime(manifest,raw())).toThrow();
    expect(()=>promotionRuntime({...manifest,manifestVersion:3})).toThrow();
    expect(()=>promotionRuntime({...manifest,runtime:{kind:"rust"}})).toThrow();
  },"bun"));
  test("checks run, attempt, source and registry before publication",()=>using(({dir,manifest})=>{
    for (const input of [{...expected,commit:"c".repeat(40)},{...expected,runId:"999"},{...expected,runAttempt:"2"},{...expected,runAttempt:"01"}]) expect(()=>verifyPromotionArtifacts(dir,input)).toThrow();
    manifest.image.repository="ghcr.io/another/image";
    save(dir,"release-manifest.json",JSON.stringify(manifest));
    expect(()=>verifyPromotionArtifacts(dir,expected)).toThrow();
  }));
  test("reserialized identity refuses even with a freshly matching checksum",()=>using(({dir})=>{
    save(dir,"rust-build-identity.json",raw()+" ");
    expect(()=>verifyPromotionArtifacts(dir,expected)).toThrow("hash mismatch");
  }));
  test.each(RUST_PROMOTION_FILES)("missing or tampered %s cannot select the Bun route",name=>using(({dir})=>{
    writeFileSync(join(dir,name),readFileSync(join(dir,name),"utf8")+" ");
    expect(()=>verifyPromotionArtifacts(dir,expected)).toThrow();
    rmSync(join(dir,name));
    expect(()=>verifyPromotionArtifacts(dir,expected)).toThrow();
  }));
  test("checksum paths and symlinks cannot redirect candidate evidence",()=>using(({dir})=>{
    writeFileSync(join(dir,"rust-package-evidence.json.sha256"),`${sha(readFileSync(join(dir,"rust-package-evidence.json"),"utf8"))}  ../rust-package-evidence.json\n`);
    expect(()=>verifyPromotionArtifacts(dir,expected)).toThrow("checksum mismatch");
    rmSync(join(dir,"rust-package-evidence.json"));symlinkSync("sbom.spdx.json",join(dir,"rust-package-evidence.json"));
    expect(()=>verifyPromotionArtifacts(dir,expected)).toThrow("regular file");
  }));
  test("rehashed evidence must still equal the independently approved manifest",()=>using(({dir})=>{
    save(dir,"supply-chain-evidence.json","{}\n");
    expect(()=>verifyPromotionArtifacts(dir,expected)).toThrow("manifest evidence mismatch");
  }));
  test("workflow requalifies Rust by digest without Bun execution or rebuilding",()=>{
    const workflow=readFileSync(new URL("../../.github/workflows/release-promote.yml",import.meta.url),"utf8");
    expect(workflow).toContain("candidate/digisense-approval.json candidate/rust-build-identity.json");
    expect(workflow).toContain("python3 scripts/release/qualify-rust-candidate.py");
    expect(workflow).toContain("--source source");
    expect(workflow).toContain('cmp "$verification/supply-chain-evidence.json" candidate/supply-chain-evidence.json');
    expect(workflow).toContain('cmp "$verification/agent-discovery-coverage-report.json" candidate/agent-discovery-coverage-report.json');
    expect(workflow).toContain('runtime_assets+=("../candidate/$file" "../candidate/$file.sha256")');
    expect(workflow).toContain('environment: digisense-approval');
    expect(workflow.indexOf("qualify-rust-candidate.py")).toBeLessThan(workflow.indexOf("docker buildx imagetools create --tag"));
    expect(workflow).not.toMatch(/docker (build|compose)(?:\s|$)|uses: docker\/build-push-action/);
  });
});
