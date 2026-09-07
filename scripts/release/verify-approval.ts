#!/usr/bin/env bun
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { isReleaseProvenance } from "../../src/core/release-provenance";
import { isValidSemVer } from "../../src/core/semver";

type JsonObject = Record<string, unknown>;

function objectField(parent: JsonObject, name: string): JsonObject {
  const value = parent[name];
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
  return value as JsonObject;
}

function stringField(parent: JsonObject, name: string): string {
  const value = parent[name];
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${name} must be a non-empty string`);
  }
  return value;
}

function assertExactKeys(
  value: JsonObject,
  expected: readonly string[],
  label: string,
): void {
  const actual = Object.keys(value).sort();
  const canonical = [...expected].sort();
  if (actual.length !== canonical.length || actual.some((key, index) => key !== canonical[index])) {
    throw new Error(`${label} fields do not match the published approval schema`);
  }
}

function isRfc3339DateTime(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(Z|[+-]\d{2}:\d{2})$/.exec(value);
  if (!match) return false;
  const [year, month, day, hour, minute, second] = match.slice(1, 7).map(Number);
  if (
    year < 1 ||
    month < 1 || month > 12 ||
    hour > 23 || minute > 59 || second > 59
  ) return false;
  const leapYear = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const daysInMonth = [31, leapYear ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  if (day < 1 || day > daysInMonth[month - 1]!) return false;
  const zone = match[7]!;
  if (zone !== "Z") {
    const [offsetHour, offsetMinute] = zone.slice(1).split(":").map(Number);
    if (offsetHour > 23 || offsetMinute > 59) return false;
  }
  return true;
}

function parseJson(path: string): { raw: Buffer; value: JsonObject } {
  const raw = readFileSync(path);
  const parsed = JSON.parse(raw.toString("utf8"));
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`${path} must contain a JSON object`);
  }
  return { raw, value: parsed as JsonObject };
}

const manifestPath = process.argv[2];
const approvalPath = process.argv[3];
if (!manifestPath || !approvalPath) {
  throw new Error("usage: verify-approval.ts <release-manifest.json> <approval.json>");
}

const manifest = parseJson(manifestPath);
const approval = parseJson(approvalPath).value;
const release = objectField(manifest.value, "release");
const image = objectField(manifest.value, "image");
const workflow = objectField(manifest.value, "workflow");
const runtime = objectField(manifest.value, "runtime");
const evidence = objectField(manifest.value, "evidence");
const reviewer = objectField(approval, "reviewer");

if (manifest.value.manifestVersion !== 1) {
  throw new Error("release manifestVersion must be 1");
}
const releaseVersion = stringField(release, "version");
const releaseCommit = stringField(release, "gitCommit");
const releaseBuiltAt = stringField(release, "builtAt");
const releaseImageDigest = stringField(image, "digest");
if (!isValidSemVer(releaseVersion) || releaseVersion.includes("+")) {
  throw new Error("release manifest version must be release-safe SemVer");
}
if (!/^[0-9a-f]{40}$/i.test(releaseCommit)) {
  throw new Error("release manifest gitCommit must be a full commit id");
}
if (Number.isNaN(Date.parse(releaseBuiltAt))) {
  throw new Error("release manifest builtAt must be ISO-8601");
}
if (!/^sha256:[0-9a-f]{64}$/i.test(releaseImageDigest)) {
  throw new Error("release manifest image digest must be sha256");
}
const runtimeBunVersion = stringField(runtime, "bunVersion");
const runtimeBaseImageDigest = stringField(runtime, "baseImageDigest");
const sbomSha256 = stringField(evidence, "sbomSha256");
const supplyChainSha256 = stringField(evidence, "supplyChainSha256");
const agentDiscoverySha256 = stringField(evidence, "agentDiscoverySha256");
const cockpitEvidenceSha256 = stringField(evidence, "cockpitEvidenceSha256");
const cockpitRegressionQuerySha256 = stringField(evidence, "cockpitRegressionQuerySha256");
if (!isValidSemVer(runtimeBunVersion) || !/^sha256:[0-9a-f]{64}$/i.test(runtimeBaseImageDigest)) {
  throw new Error("release manifest runtime identity is invalid");
}
if (![sbomSha256, supplyChainSha256, agentDiscoverySha256, cockpitEvidenceSha256, cockpitRegressionQuerySha256]
  .every((digest) => /^sha256:[0-9a-f]{64}$/i.test(digest))) {
  throw new Error("release manifest evidence checksums are invalid");
}
if (
  !/^\d+$/.test(stringField(workflow, "runId")) ||
  typeof workflow.runAttempt !== "number" ||
  !Number.isSafeInteger(workflow.runAttempt) ||
  workflow.runAttempt < 1
) {
  throw new Error("release manifest workflow run identity is invalid");
}
if (!isReleaseProvenance(manifest.value.provenance)) {
  throw new Error("release manifest provenance is invalid");
}
if (
  manifest.value.provenance.product.version !== releaseVersion ||
  manifest.value.provenance.product.gitCommit !== releaseCommit ||
  manifest.value.provenance.product.builtAt !== releaseBuiltAt
) {
  throw new Error("release manifest provenance does not match release identity");
}
if (
  manifest.value.provenance.product.bunVersion !== runtimeBunVersion ||
  manifest.value.provenance.product.baseImageDigest !== runtimeBaseImageDigest
) {
  throw new Error("release manifest runtime does not match provenance");
}

assertExactKeys(
  approval,
  [
    "schemaVersion",
    "approvalId",
    "decision",
    "approvedAt",
    "reviewer",
    "releaseManifestDigest",
    "imageDigest",
    "version",
    "gitCommit",
  ],
  "approval",
);
assertExactKeys(reviewer, ["organization", "name"], "approval.reviewer");
if (approval.schemaVersion !== 1) throw new Error("approval.schemaVersion must be 1");
if (approval.decision !== "approved") throw new Error("approval.decision must be 'approved'");
if (stringField(reviewer, "organization") !== "Digisense") {
  throw new Error("approval reviewer organization must be Digisense");
}
stringField(reviewer, "name");
stringField(approval, "approvalId");
const approvedAt = stringField(approval, "approvedAt");
if (!isRfc3339DateTime(approvedAt)) throw new Error("approvedAt must be RFC 3339 date-time");
if (Date.parse(approvedAt) < Date.parse(releaseBuiltAt)) {
  throw new Error("approvedAt cannot be earlier than the candidate build");
}

const manifestDigest = `sha256:${createHash("sha256").update(manifest.raw).digest("hex")}`;
const exactMatches: Array<[string, string, string]> = [
  ["releaseManifestDigest", stringField(approval, "releaseManifestDigest"), manifestDigest],
  ["imageDigest", stringField(approval, "imageDigest"), releaseImageDigest],
  ["version", stringField(approval, "version"), releaseVersion],
  ["gitCommit", stringField(approval, "gitCommit"), releaseCommit],
];
if (!/^sha256:[0-9a-f]{64}$/.test(stringField(approval, "releaseManifestDigest"))) {
  throw new Error("releaseManifestDigest must match the published schema");
}
if (!/^sha256:[0-9a-f]{64}$/.test(stringField(approval, "imageDigest"))) {
  throw new Error("imageDigest must match the published schema");
}
if (!isValidSemVer(stringField(approval, "version")) || stringField(approval, "version").includes("+")) {
  throw new Error("approval version must match the published release SemVer schema");
}
if (!/^[0-9a-f]{40}$/.test(stringField(approval, "gitCommit"))) {
  throw new Error("approval gitCommit must match the published schema");
}
for (const [field, actual, expected] of exactMatches) {
  if (actual !== expected) throw new Error(`${field} does not match the release manifest`);
}

process.stdout.write(
  `verified ${stringField(approval, "approvalId")}: Digisense approved ${releaseVersion} at ${releaseImageDigest}\n`,
);
