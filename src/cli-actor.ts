import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { companyPaths } from "./core/paths";

export const MUTATING_COMMANDS = new Set([
  "customer create",
  "customer validate-vat",
  "vendor create",
  "system backup",
  "system backup-archive",
  "system backup-add-destination",
  "system backup-remove-destination",
  "system backup-place",
  "system backup-confirm-placement",
  "system backup-lock",
  "system restore-backup",
  "system export-authority",
  "system export-accountant",
  "invoice issue",
  // #265: `invoice create` is the guided path that issues a real, locked,
  // immutable invoice through the SAME core as `invoice issue` — it MUST be
  // gated by the actor allowlist exactly like `invoice issue`.
  "invoice create",
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
  // ===== BANK CLUSTER (#187) =====
  "bank-account add",
  // ===== END BANK CLUSTER (#187) =====
  "expense book",
  "vat post-eu-service-purchase",
  "vat post-representation-purchase",
  "period close",
  "period reopen",
  "journal post",
  "journal reverse",
  "exceptions resolve",
  // ===== RECURRING INVOICES (#118) =====
  "recurring-invoice create",
  "recurring-invoice generate",
  // ===== END RECURRING INVOICES (#118) =====
  // ===== MAIL INTAKE (#122) =====
  "mail-intake ingest",
  // ===== IMAP INTAKE (#181) =====
  "imap-intake poll",
  // ===== END IMAP INTAKE (#181) =====
  // ===== MILEAGE LOG (#123) =====
  "mileage log",
  "mileage export",
  // Fixed assets (#124, #125)
  "asset register",
  "asset depreciate",
  "asset write-off",
  // PEPPOL submission (#128)
  "invoice submit-public-peppol",
  // ===== OPENING BALANCE (#179) =====
  "opening-balance post",
  // ===== END OPENING BALANCE (#179) =====
  // ===== EMAIL DELIVERY (#180) =====
  "invoice send",
  // ===== END EMAIL DELIVERY (#180) =====
  // ===== GDPR (#184) =====
  "gdpr erase",
  // ===== END GDPR (#184) =====
  // ===== IMPORT FRAMEWORK (#185) =====
  "import run",
  // ===== END IMPORT FRAMEWORK (#185) =====
  // ===== RUNTIME AGENT (#183) =====
  "agent run",
  // ===== END RUNTIME AGENT (#183) =====
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
    trimToNull(process.env.LOGNAME ? `user:${process.env.LOGNAME}` : null) ??
    trimToNull(process.env.USERNAME ? `user:${process.env.USERNAME}` : null)
  );
}

/**
 * #248: the actor-allowlist section of `policy.yaml` describes who may run
 * mutating commands. The allowlist's section keys (`users:`/`agents:`/
 * `systems:`) follow from the actor's `kind:` prefix, so the hint can name the
 * exact line a user needs to add.
 */
function howToAddActorHint(actor: string): string {
  const [kind] = actor.split(":", 1);
  const section =
    kind === "agent" ? "agents" : kind === "system" ? "systems" : "users";
  return (
    `Tilføj '${actor}' under actor_allowlist.${section} i config/policy.yaml ` +
    `(linjen '    - ${actor}'), eller kør med en allerede tilladt --actor.`
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
    // #283: `system restore-backup` writes to `--target-company`, a path that
    // is normally brand new (the whole point of a restore is to recreate a
    // company from a backup). The allowlist lives in
    // `<target>/config/policy.yaml`, which a not-yet-restored target cannot
    // possibly have yet. Enforcing the allowlist against that absent file
    // rejects EVERY explicit `--actor` — even a correctly allowlisted one —
    // while a derived actor (no `--actor`) slips through, so doing the right
    // thing is blocked and doing less works. A fresh restore has no policy to
    // enforce against, so the allowlist check is skipped here; the canonical
    // format check above still applies, and a restore into an EXISTING
    // company (which does have a policy file) is still fully enforced below.
    if (
      commandKey === "system restore-backup" &&
      !existsSync(join(companyPaths(root).config, "policy.yaml"))
    ) {
      process.env.RENTEMESTER_ACTOR = explicitActor;
      if (cliActorVia) process.env.RENTEMESTER_ACTOR_VIA = cliActorVia;
      else if (!trimToNull(process.env.RENTEMESTER_ACTOR_VIA))
        process.env.RENTEMESTER_ACTOR_VIA = "rentemester-cli";
      return;
    }
    const allowlist = loadActorAllowlist(root);
    if (!allowlist.has(explicitActor)) {
      fatal(
        `actor '${explicitActor}' is not in config/policy.yaml actor_allowlist. ` +
          howToAddActorHint(explicitActor),
      );
    }
    process.env.RENTEMESTER_ACTOR = explicitActor;
    if (cliActorVia) process.env.RENTEMESTER_ACTOR_VIA = cliActorVia;
    else if (!trimToNull(process.env.RENTEMESTER_ACTOR_VIA))
      process.env.RENTEMESTER_ACTOR_VIA = "rentemester-cli";
    return;
  }
  // No explicit --actor: the entry is attributed to a derived actor (OS
  // username / agent env var). #248: onboarding (`init` / `company add`) seeds
  // this derived actor into config/policy.yaml's actor_allowlist, so for the
  // person who set the company up the derived actor and an explicit `--actor`
  // with the same id behave identically — the allowlist is consistent. The
  // derived fallback itself stays lenient (it is the no-friction default for
  // scripts and agents); when the derived actor differs from what onboarding
  // seeded, `howToAddActorHint` on the explicit path explains how to add it.
  const derivedActor = inferredMutationActor();
  if (!derivedActor) {
    fatal(
      "actor required for mutations: pass --actor <user:...|agent:...|system:...> or run with USER/LOGNAME/OPENCLAW_AGENT set",
    );
  }
}
