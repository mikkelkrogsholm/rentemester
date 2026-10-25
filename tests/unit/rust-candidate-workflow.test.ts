import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { assertReproducedImage, buildArgs, checksum, buildCommand, identity, immutableImage, publishedPlatformManifest, qualify, qualifierCommand, reproducibility, sbomTemplate } from "../../scripts/release/rust-candidate";
const root = join(import.meta.dir,"../..");
const workflow = readFileSync(join(root,".github/workflows/release-candidate.yml"),"utf8");
const runner = readFileSync(join(root,"scripts/release/rust-candidate.ts"),"utf8");
const env = { RELEASE_VERSION:"1.2.3",RELEASE_GIT_COMMIT:"a".repeat(40),SOURCE_DATE_EPOCH:"1767225600",RELEASE_BUILT_AT:"2026-01-01T00:00:00Z",GITHUB_RUN_ID:"123",GITHUB_RUN_ATTEMPT:"2" };
const current = identity(env);
describe("original candidate Rust v2 branch",()=>{
 test("Rust is explicit default; every legacy evidence/manifest/smoke route remains Bun-only",()=>{
  expect(workflow).toContain("default: rust");expect(workflow).toContain("options: [rust, bun]");
  for(const name of ["Create lockfile-bound supply-chain evidence","Verify reproducible OCI export","Verify container readiness and persisted restart","Smoke the published candidate digest","Render the published local-container cockpit","Extract digest-bound SPDX SBOM evidence","Verify packaged agent-discovery coverage identity","Create digest-bound release manifest"]){expect(workflow).toContain(`- name: ${name}\n        if: inputs.runtime == 'bun'`);}
  expect(workflow).toContain("file: ${{ inputs.runtime == 'rust' && 'Dockerfile.rust' || 'Dockerfile' }}");
  expect(workflow).not.toContain("continue-on-error:");expect(workflow).toContain("if: always() && inputs.runtime == 'rust'");
 });
 test("current main and immutable run-attempt identity precede builds",()=>{
  expect(workflow).toContain('if [ "$commit" != "$GITHUB_SHA" ]');expect(workflow).toContain('[ "$commit" = "$(git rev-parse origin/main)" ]');
  expect(workflow).toContain('${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}');
  expect(workflow).toContain('[ "$registry_status" = 404 ]');expect(workflow).not.toContain(":latest");
  for(const [key,value]of [["RELEASE_GIT_COMMIT","a".repeat(39)],["SOURCE_DATE_EPOCH","0"],["GITHUB_RUN_ATTEMPT","-1"],["RELEASE_BUILT_AT","2026-01-01T00:00:01Z"],["RELEASE_VERSION","1.2.3+unreviewed"]])expect(()=>identity({...env,[key!]:value})).toThrow();
 });
 test("the actual source preflight rejects untracked compile inputs before claiming clean source",()=>{
  const parsed=Bun.YAML.parse(workflow) as {jobs:{candidate:{steps:Array<{name:string;run?:string}>}}};
  const gate=parsed.jobs.candidate.steps.find(s=>s.name==="Verify clean candidate source")!.run!;
  const dir=mkdtempSync(join(tmpdir(),"rust-candidate-source-"));
  try{
   expect(Bun.spawnSync(["git","init","--quiet"],{cwd:dir}).exitCode).toBe(0);
   writeFileSync(join(dir,"Cargo.toml"),"# untracked compile input\n");
   expect(Bun.spawnSync(["bash","-c",gate],{cwd:dir,stdout:"pipe",stderr:"pipe"}).exitCode).not.toBe(0);
   unlinkSync(join(dir,"Cargo.toml"));
   expect(Bun.spawnSync(["bash","-c",gate],{cwd:dir,stdout:"pipe",stderr:"pipe"}).exitCode).toBe(0);
  }finally{rmSync(dir,{recursive:true,force:true});}
 });
 test("independent OCI builds cannot reuse compiler caches or silently target local arm64",()=>{
  const first=buildCommand(current,"first","first-builder","type=oci,dest=first.tar,rewrite-timestamp=true");
  const second=buildCommand(current,"second","second-builder","type=oci,dest=second.tar,rewrite-timestamp=true");
  for(const command of [first,second]){expect(command).toContain("Dockerfile.rust");expect(command).toContain("linux/amd64");expect(command).toContain("--no-cache");expect(command).toContain("--provenance=false");expect(command).toContain("--sbom=false");expect(command).toContain("RENTEMESTER_SOURCE_DIRTY=false");}
  expect(first).toContain("RUST_BUILD_CACHE_ID=release-123-2-first");expect(second).toContain("RUST_BUILD_CACHE_ID=release-123-2-second");expect(buildArgs(current,"publisher")).toContain("RUST_BUILD_CACHE_ID=release-123-2-publisher");expect(()=>buildArgs(current,"shared")).toThrow();
  expect(runner).toContain('hashes[0] !== hashes[1] || digests[0] !== digests[1]');expect(runner).toContain('"docker-container"');
 });
 test("publication must match the cold proof's exact image config and amd64 platform",()=>{
  const config="sha256:"+"d".repeat(64);const proof={status:"passed",platform:"linux/amd64",configDigest:config};const image={Id:config,Os:"linux",Architecture:"amd64"};
  expect(()=>assertReproducedImage(proof,image)).not.toThrow();
  for(const changed of [{...image,Id:"sha256:"+"e".repeat(64)},{...image,Architecture:"arm64"},{...image,Os:"windows"}])expect(()=>assertReproducedImage(proof,changed)).toThrow();
  expect(workflow).toContain("RUST_BUILD_CACHE_ID=release-${{ github.run_id }}-${{ github.run_attempt }}-first");
  expect(workflow).toContain("format('rm-rust-{0}-{1}-first', github.run_id, github.run_attempt)");
  expect(workflow).toContain("timeout-minutes: 120");
 });
 test("qualification never accepts mutable tags, wrong repositories or missing OCI digest",()=>{
  expect(immutableImage("ghcr.io/mikkelkrogsholm/rentemester","sha256:"+"b".repeat(64))).toEndWith("@sha256:"+"b".repeat(64));
  for(const d of [undefined,"candidate-v1","sha256:"+"b".repeat(63)])expect(()=>immutableImage("ghcr.io/mikkelkrogsholm/rentemester",d)).toThrow();
  expect(()=>immutableImage("example.invalid/foreign","sha256:"+"b".repeat(64))).toThrow();
 });
 test("published platform manifest selection rejects extra native architectures and untyped hidden images",()=>{
  const digest="sha256:"+"f".repeat(64);const image={digest,platform:{os:"linux",architecture:"amd64"}};
  const attestation={digest:"sha256:"+"e".repeat(64),platform:{os:"unknown",architecture:"unknown"},annotations:{"vnd.docker.reference.type":"attestation-manifest"}};
  expect(publishedPlatformManifest({manifests:[image,attestation]},"root")).toBe(digest);
  expect(publishedPlatformManifest({config:{}},digest)).toBe(digest);
  expect(()=>publishedPlatformManifest({manifests:[image,{...image,platform:{os:"linux",architecture:"arm64"}}]},"root")).toThrow();
  expect(()=>publishedPlatformManifest({manifests:[image,{digest}]},"root")).toThrow();
  expect(workflow).toContain("',oci-mediatypes=true'");
 });
 test("BuildKit SPDX selection uses the exact platform or unambiguous single-platform document",()=>{
  expect(sbomTemplate(JSON.stringify({"linux/amd64":{SPDX:{}}}))).toBe('{{ json (index .SBOM "linux/amd64").SPDX }}');
  expect(sbomTemplate('{"SPDX":{}}')).toBe("{{ json .SBOM.SPDX }}");
  for(const raw of ["null","[]","{}",'{"linux/arm64":{"SPDX":{}}}','{"SPDX":{},"linux/arm64":{}}'])expect(()=>sbomTemplate(raw)).toThrow();
  expect(runner).toContain('newFile("sbom.spdx.json", raw)');expect(runner).not.toContain('JSON.stringify(spdx)');
 });
 test("full original qualifier gets all exact frozen witnesses without native-count fallback",()=>{
  const image=immutableImage("ghcr.io/mikkelkrogsholm/rentemester","sha256:"+"b".repeat(64));
  expect(qualifierCommand(image,current)).toEqual(["python3","scripts/release/qualify-rust-candidate.py","--source",".","--image",image,"--identity","rust-build-identity.json","--package-evidence","rust-package-evidence.json","--license-evidence","rust-license-evidence.json","--sbom","sbom.spdx.json","--commit","a".repeat(40),"--version","1.2.3","--platform","linux/amd64","--output","supply-chain-evidence.json","--discovery-output","agent-discovery-coverage-report.json"]);
  expect(runner.indexOf("execute(qualifierCommand(image, i))")).toBeLessThan(runner.indexOf('newFile("release-manifest.json"'));
  expect(runner).toContain('"--require-clean"');expect(runner).toContain('flag: "wx"');expect(runner).toContain('"scripts/release/create-rust-manifest.ts", "rust-build-identity.json"');
 });
 test("a failed cold build stops without a second build or a Bun retry",async()=>{
  const calls:string[][]=[];
  await expect(reproducibility(current,(args)=>{calls.push(args);if(args[2]==="build")throw new Error("guarded cold build refused");return Buffer.alloc(0);})).rejects.toThrow("guarded cold build refused");
  expect(calls.filter(a=>a[0]==="docker"&&a[2]==="build")).toHaveLength(1);
  expect(calls.some(a=>a.includes("Dockerfile"))).toBe(false);
 });
 test("immutable package lookup failure prevents qualifier and manifest creation",()=>{
  const calls:string[][]=[];expect(()=>qualify(current,"ghcr.io/mikkelkrogsholm/rentemester@sha256:"+"b".repeat(64),(args)=>{calls.push(args);throw new Error("registry unavailable");})).toThrow("registry unavailable");
  expect(calls).toHaveLength(1);expect(calls[0]?.slice(0,2)).toEqual(["docker","pull"]);
 });
 test("qualifier-owned sidecars are verified and preserved, never overwritten",()=>{
  const dir=mkdtempSync(join(tmpdir(),"rust-candidate-sidecar-"));const path=join(dir,"supply-chain-evidence.json");
  try{
   writeFileSync(path,"exact raw report\n");
   const hash=checksum(path);const expected=`${hash.slice(7)}  supply-chain-evidence.json\n`;
   expect(readFileSync(path+".sha256","utf8")).toBe(expected);
   expect(checksum(path)).toBe(hash);expect(readFileSync(path+".sha256","utf8")).toBe(expected);
   const wrong=expected.replace("supply-chain-evidence.json","foreign.json");writeFileSync(path+".sha256",wrong);
   expect(()=>checksum(path)).toThrow("existing evidence checksum");expect(readFileSync(path+".sha256","utf8")).toBe(wrong);
   expect(readFileSync(path,"utf8")).toBe("exact raw report\n");
  }finally{rmSync(dir,{recursive:true,force:true});}
 });
 test("same original artifact carries raw Rust identity, package and complete license witnesses",()=>{
  expect(workflow.match(/name: release-candidate-evidence/g)).toHaveLength(1);
  for(const name of ["rust-build-identity.json","rust-package-evidence.json","rust-license-evidence.json","sbom.spdx.json","supply-chain-evidence.json","agent-discovery-coverage-report.json","release-manifest.json"]){expect(workflow).toContain(`            ${name}\n`);expect(workflow).toContain(`${name}.sha256`);}
  expect(workflow.indexOf("name: Qualify exact Rust package, SPDX and full original discovery")).toBeLessThan(workflow.indexOf("name: Attest candidate image provenance"));
  expect(workflow.indexOf("name: Attest candidate image provenance")).toBeLessThan(workflow.indexOf("name: Upload candidate evidence for Digisense"));
 });
 test("one physical lifecycle precedes publication; published unchanged bytes are compared exactly",()=>{
  expect(workflow.indexOf("name: Verify Rust readiness, normal owner and persisted restart")).toBeLessThan(workflow.indexOf("name: Build and push candidate image"));
  expect(runner).toContain('"scripts/rust/run-rust-container.py"');expect(runner).toContain('"rust-container-preflight.json"');expect(runner.match(/scripts\/rust\/run-rust-container.py/g)).toHaveLength(1);expect(runner).toContain("reproduced(image, true)");expect(runner).toContain("proof.digest");
  expect(workflow).toContain("playwright install --with-deps chromium");expect(workflow).toContain("sbom: true");
  expect(runner).not.toContain('"--push"');expect(runner).not.toContain('"imagetools", "create"');
 });
});
