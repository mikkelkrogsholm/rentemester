#!/usr/bin/env bun
import { verifyProductionLicenses } from "./check-production-licenses";

function sha256(bytes: Uint8Array): string {
  return new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
}

export function createSupplyChainEvidence(input: {
  auditReport: unknown;
  licenseReport: unknown;
  lockfileBytes: Uint8Array;
  bunVersion: string;
}) {
  if (!input.auditReport || typeof input.auditReport !== "object" || Array.isArray(input.auditReport)) {
    throw new Error("dependency audit report must be an object");
  }
  const advisoryCount = Object.values(input.auditReport).reduce((count, advisories) => {
    if (!Array.isArray(advisories)) throw new Error("dependency audit report is malformed");
    return count + advisories.length;
  }, 0);
  if (advisoryCount !== 0) throw new Error(`dependency audit contains ${advisoryCount} advisories`);
  if (!/^\d+\.\d+\.\d+$/.test(input.bunVersion)) throw new Error("Bun version is invalid");

  const licenses = verifyProductionLicenses(input.licenseReport);
  return {
    evidenceVersion: 1,
    bunVersion: input.bunVersion,
    lockfile: { path: "bun.lock", sha256: `sha256:${sha256(input.lockfileBytes)}` },
    audit: { advisoryCount, report: input.auditReport },
    licenses: { packageCount: licenses.packages, allowlist: licenses.licenses, report: input.licenseReport },
  };
}

function runJsonCommand(args: string[]): unknown {
  const result = Bun.spawnSync([process.execPath, ...args], { stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(`${args.join(" ")} failed`);
  return JSON.parse(result.stdout.toString());
}

async function readAuditReport(): Promise<unknown> {
  const path = process.env.SUPPLY_CHAIN_AUDIT_REPORT_PATH;
  if (!path) return runJsonCommand(["audit", "--json"]);
  try {
    return JSON.parse(await Bun.file(path).text());
  } catch {
    throw new Error(`audit report ${path} is unreadable or malformed`);
  }
}

if (import.meta.main) {
  const evidence = createSupplyChainEvidence({
    auditReport: await readAuditReport(),
    licenseReport: runJsonCommand(["pm", "licenses", "--prod", "--json"]),
    lockfileBytes: new Uint8Array(await Bun.file("bun.lock").arrayBuffer()),
    bunVersion: Bun.version,
  });
  process.stdout.write(`${JSON.stringify(evidence, null, 2)}\n`);
}
