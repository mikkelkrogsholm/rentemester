import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { companyPaths } from "./core/paths";

export const MUTATING_COMMANDS = new Set([
  "customer create",
  "customer validate-vat",
  "vendor create",
  "system backup",
  "system restore-backup",
  "system export-authority",
  "invoice issue",
  "invoice render",
  "invoice credit-note",
  "invoice post",
  "invoice settle-bank",
  "invoice settle-claim-bank",
  "invoice write-off-bad-debt",
  "invoice refund-bank",
  "invoice apply-payment",
  "invoice remind",
  "invoice post-reminder",
  "invoice claim-interest",
  "invoice post-interest",
  "invoice claim-compensation",
  "invoice post-compensation",
  "documents ingest",
  "bank import",
  "expense book",
  "vat post-eu-service-purchase",
  "vat post-representation-purchase",
  "period close",
  "journal post",
  "journal reverse",
  "exceptions resolve",
]);

export function trimToNull(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function isCanonicalActorId(value: string): boolean {
  return /^(user|agent|system):\S.+$/.test(value);
}

export function loadActorAllowlist(root: string): Set<string> {
  const policyPath = join(companyPaths(root).config, "policy.yaml");
  if (!existsSync(policyPath)) return new Set<string>();
  const allowlist = new Set<string>();
  let inActorAllowlist = false;
  let section: string | null = null;
  for (const rawLine of readFileSync(policyPath, "utf8").split(/\r?\n/)) {
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const indent = rawLine.match(/^\s*/)?.[0].length ?? 0;
    if (!inActorAllowlist) {
      if (trimmed === "actor_allowlist:") inActorAllowlist = true;
      continue;
    }
    if (indent === 0) break;
    if (indent === 2 && trimmed.endsWith(":")) {
      section = trimmed.slice(0, -1);
      continue;
    }
    const item = rawLine.match(/^\s*-\s*(.+?)\s*$/)?.[1]?.trim();
    if (!item) continue;
    const value = item.replace(/^['"]|['"]$/g, "");
    if (section === "users") allowlist.add(value.startsWith("user:") ? value : `user:${value}`);
    else if (section === "agents")
      allowlist.add(value.startsWith("agent:") ? value : `agent:${value}`);
    else if (section === "systems")
      allowlist.add(value.startsWith("system:") ? value : `system:${value}`);
    else allowlist.add(value);
  }
  return allowlist;
}

export function inferredMutationActor(): string | null {
  return (
    trimToNull(process.env.OPENCLAW_AGENT ? `agent:${process.env.OPENCLAW_AGENT}` : null) ??
    trimToNull(process.env.RENTEMESTER_AGENT ? `agent:${process.env.RENTEMESTER_AGENT}` : null) ??
    trimToNull(process.env.RENTEMESTER_USER ? `user:${process.env.RENTEMESTER_USER}` : null) ??
    trimToNull(process.env.USER ? `user:${process.env.USER}` : null) ??
    trimToNull(process.env.LOGNAME ? `user:${process.env.LOGNAME}` : null)
  );
}

export function enforceMutationActorPolicy(
  commandKey: string,
  root: string,
  cliActor: string | null,
  cliActorVia: string | null,
  fatal: (message: string) => never,
): void {
  if (!MUTATING_COMMANDS.has(commandKey)) return;
  const explicitActor = cliActor ?? trimToNull(process.env.RENTEMESTER_ACTOR);
  if (explicitActor) {
    if (!isCanonicalActorId(explicitActor)) {
      fatal("explicit actor must use canonical format user:<id>, agent:<id>, or system:<id>");
    }
    const allowlist = loadActorAllowlist(root);
    if (!allowlist.has(explicitActor)) {
      fatal(
        `actor '${explicitActor}' is not in config/policy.yaml actor_allowlist; add it or run without --actor`,
      );
    }
    process.env.RENTEMESTER_ACTOR = explicitActor;
    if (cliActorVia) process.env.RENTEMESTER_ACTOR_VIA = cliActorVia;
    else if (!trimToNull(process.env.RENTEMESTER_ACTOR_VIA))
      process.env.RENTEMESTER_ACTOR_VIA = "rentemester-cli";
    return;
  }
  if (!inferredMutationActor()) {
    fatal(
      "actor required for mutations: pass --actor <user:...|agent:...|system:...> or run with USER/LOGNAME/OPENCLAW_AGENT set",
    );
  }
}
