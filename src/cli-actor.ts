import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { companyPaths } from "./core/paths";

export const MUTATING_COMMANDS = new Set([
  "workspace snapshot",
  "workspace restore",
  "workspace-access bootstrap-first",
  "workspace-access bootstrap-local-service",
  "workspace-access local-service-rotate",
  "workspace-access local-service-revoke",
  "group apply-manifest",
  "group propose-mapping",
  "group approve-mapping",
  "group revoke-mapping",
  "group propose-elimination",
  "group approve-elimination",
  "group reject-elimination",
  "group apply-elimination",
  "group reverse-elimination",
  "group propose-profile",
  "group approve-profile",
  "group revoke-profile",
  "group propose-disposition",
  "group approve-disposition",
  "group link-disposition",
  "group settle-disposition",
  "group reopen-disposition",
  "group supersede-disposition",
  "efaktura onboard",
  "accounts add",
  "accounts role-confirm",
  "customer create",
  "customer validate-vat",
  "vendor create",
  // Audit 2026-06-11 (AGENT-2): `company sync-cvr` skriver CVR-stamdata til
  // company-tabellen — MCP-pendanten `company_sync_cvr` er write-reversible
  // + confirm-gated, så CLI-navnet skal være actor-gated, ikke read-only.
  "company sync-cvr",
  // Audit 2026-06-11 (AGENT-3): `company set-profile` skriver navn, CVR,
  // adresse, payment_terms_days, bank og VAT-periode til company-db'en via
  // setCompanyProfile + setCompanyVatPeriodType (src/cli/company.ts) — samme
  // bug-klasse som `company sync-cvr`. Den skal være actor-gated.
  "company set-profile",
  "system backup",
  "system migrate",
  "system repair-schema-views",
  "system backup-archive",
  "system backup-add-destination",
  "system backup-remove-destination",
  "system backup-place",
  "system backup-confirm-placement",
  "system backup-verify-remote-placement",
  "system backup-lock",
  "system restore-backup",
  "system export-authority",
  "system export-accountant",
  // Audit 2026-06-11 (AGENT-3): `system export-saft` skriver en
  // `saft_export`-række til audit_log (insertAuditLog i src/core/saft-export.ts)
  // — præcis som export-authority/export-accountant ovenfor, der allerede er
  // gated. Samme actor-attribuerede skrivning skal gates ens.
  "system export-saft",
  "invoice issue",
  "invoice imported-receivables-backfill-apply",
  "invoice imported-receivable-settlement-apply",
  "bank legacy-binding-apply",
  "bank legacy-payable-backfill-apply",
  // #265: `invoice create` is the guided path that issues a real, locked,
  // immutable invoice through the SAME core as `invoice issue` — it MUST be
  // gated by the actor allowlist exactly like `invoice issue`.
  "invoice create",
  "invoice render",
  "invoice credit-note",
  "invoice post",
  "invoice repair-posting",
  "invoice settle-bank",
  "invoice settle-claim-bank",
  "invoice write-off-bad-debt",
  "invoice refund-bank",
  "invoice apply-payment",
  "invoice remind",
  "invoice post-reminder",
  "invoice claim-interest",
  "invoice post-interest",
  "invoice post-interest-correction",
  "invoice claim-compensation",
  "invoice post-compensation",
  "documents ingest",
  "documents enrich",
  "documents set-company-context",
  "documents party-link-apply",
  "documents party-link-supersede",
  "documents extract-invoice",
  "documents parse",
  "documents parse-pending",
  "bank import",
  "bank link-journal",
  "bank correction-apply",
  "bank direct-payable-apply",
  // ===== BANK CLUSTER (#187) =====
  "bank-account add",
  "bank-account update",
  // ===== END BANK CLUSTER (#187) =====
  "expense book",
  "expense vat-preflight",
  "bookkeeping-batch dry-run",
  "bookkeeping-batch persist",
  "bookkeeping-batch approve",
  "bookkeeping-batch apply",
  "purchase-case create",
  "purchase-case review",
  "purchase-case reassess",
  "purchase-case group-review",
  "approval-policy set",
  "party create",
  "party link-role",
  "party propose-merge",
  "party approve-merge",
  "corporate-record ingest",
  "corporate-record link",
  "corporate-record enrich",
  "corporate-record supersede",
  "ownership propose",
  "ownership review",
  "ownership apply",
  "vat post-eu-service-purchase",
  "vat post-representation-purchase",
  "period close",
  "period review",
  "period reopen",
  "journal post",
  "journal reverse",
  "accounting-draft create",
  "accounting-draft revise",
  "accounting-draft submit",
  "accounting-draft reject",
  "accounting-draft approve-and-post",
  "exceptions resolve",
  // ===== RECURRING INVOICES (#118) =====
  "recurring-invoice create",
  "recurring-invoice generate",
  "recurring-invoice run-workspace",
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
  // Digisense e-faktura-transport (#efaktura): `invoice transmit-digisense`
  // udfører en LIVE afsendelse (validate-document -> deliver-document -> poll)
  // og bogfører en `acknowledged` peppol_submissions-række + audit_log — præcis
  // samme bug-klasse som `invoice submit-public-peppol` ovenfor. Uden denne
  // gate listes den under "Læsekommandoer (read-only)" i --help og kan kaldes
  // uden actor-attribution, i modstrid med governance-modellen for alle andre
  // skrivende invoice-kommandoer (en uigenkaldelig afsendelse til en rigtig
  // modtager).
  "invoice transmit-digisense",
  // ===== OPENING BALANCE (#179) =====
  "opening-balance post",
  // ===== END OPENING BALANCE (#179) =====
  // ===== EMAIL DELIVERY (#180) =====
  "invoice send",
  // ===== END EMAIL DELIVERY (#180) =====
  // ===== GDPR (#184) =====
  // Discovery and export append immutable audit_log events, so they are
  // mutations even though the subject data itself is only read.
  "gdpr discover",
  "gdpr export",
  "gdpr erase",
  // Audit 2026-06-11 (AGENT-1): `gdpr forget` er det kanoniske navn for
  // samme runEraser som legacy-aliaset `gdpr erase` — et alias og dets
  // kanoniske navn SKAL have samme governance-klasse, ellers kan den ene
  // stavemåde mutere uden actor-gate (og listes som read-only i hjælpen).
  "gdpr forget",
  // ===== END GDPR (#184) =====
  // ===== IMPORT FRAMEWORK (#185) =====
  "import run",
  // Audit 2026-06-11 (AGENT-3): `import contacts` lander en Dinero-kontakt-CSV
  // i customer/vendor-master-data via createCustomer/createVendor
  // (src/core/import/dinero-contacts.ts → insertAuditLog). De enkeltvise
  // `customer create`/`vendor create` er gated, så bulk-import-stien skal
  // også være det.
  "import contacts",
  // ===== END IMPORT FRAMEWORK (#185) =====
  // ===== RUNTIME AGENT (#183) =====
  "agent run",
  // ===== END RUNTIME AGENT (#183) =====
  // ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
  "accrual register",
  "accrual recognize",
  // ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
  // ===== BUDGET =====
  "budget set",
  // Accounting dimensions are append-only ledger/master-data writes.  Keep
  // every mutating subcommand here so the central actor gate, help and agent
  // discovery cannot accidentally describe one as read-only.
  "dimensions define",
  "dimensions member",
  "dimensions definition-lifecycle",
  "dimensions member-lifecycle",
  "dimensions apply",
  "dimensions replace",
  "dimensions supersede",
  "dimensions budget-apply",
  // ===== END BUDGET =====
  // ===== PAYABLES / KREDITORSTYRING =====
  "payable register",
  "payable pay",
  "posting-rules propose",
  "posting-rules approve",
  "posting-rules disable",
  "posting-rules supersede",
  // ===== END PAYABLES / KREDITORSTYRING =====
  // ===== DIGISENSE E-FAKTURA (#efaktura) =====
  // `efaktura registrer` skriver virksomheds-/participant-state + audit_log til
  // ledgeren og rammer netværket (register-company). `efaktura modtag` ingester
  // modtagne bilag (append-only dokumenter + dedup-rækker + audit_log) og rammer
  // netværket. Begge er skrivende handlinger og skal — som alle andre — kræve en
  // actor og listes under "Skrivekommandoer", ikke under "read-only".
  "efaktura registrer",
  "efaktura registrer-test-gln",
  "efaktura registrer-test-afsender",
  "efaktura konfigurer",
  "efaktura modtag",
  "efaktura modtag-workspace",
  "efaktura status",
  // ===== END DIGISENSE E-FAKTURA (#efaktura) =====
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

/** Explicit, local policy for the exceptional forced period-close waiver. */
export function actorMayForcePeriodClose(root: string, actor: string | null | undefined): boolean {
  if (!actor) return false;
  const policyPath = join(companyPaths(root).config, "policy.yaml");
  if (!existsSync(policyPath)) return false;
  let inSection = false;
  for (const rawLine of readFileSync(policyPath, "utf8").split(/\r?\n/)) {
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    if (trimmed === "period_close_force_actors:") { inSection = true; continue; }
    if (inSection && !/^\s/.test(rawLine)) break;
    if (!inSection) continue;
    const value = rawLine.match(/^\s*-\s*(.+?)\s*$/)?.[1]?.replace(/^['"]|['"]$/g, "");
    if (value && normaliseActorForMatching(value) === normaliseActorForMatching(actor)) return true;
  }
  return false;
}

/**
 * #248 (follow-up): the allowlist comparison normalises case + surrounding
 * whitespace so that an explicit `--actor` form and its derived (USER) twin
 * — two spellings of the same identity — produce the same allowlist hit. On
 * macOS/Linux usernames are case-sensitive, but on a CLI the difference
 * between `--actor user:Mikkel` and a USER=mikkel environment is purely
 * incidental: the audit-trail identity is the same person. Without
 * normalisation the explicit path rejected `user:mikkel` while the derived
 * path silently accepted `user:Mikkel`, breaking the allowlist's central
 * promise that one rule applies to both forms.
 *
 * The normalisation is matching-only: the original spelling (whatever the
 * caller passed) is still what flows into `RENTEMESTER_ACTOR` and the audit
 * log, so the ledger keeps an honest record of what was typed. This is also
 * NOT a relaxation of security — a name that has no case-insensitive twin
 * in the allowlist is still rejected on both paths.
 */
function normaliseActorForMatching(actor: string): string {
  return actor.trim().toLowerCase();
}

export function actorMatchesAllowlist(
  actor: string,
  allowlist: Set<string>,
): boolean {
  if (allowlist.has(actor)) return true;
  const normalised = normaliseActorForMatching(actor);
  for (const entry of allowlist) {
    if (normaliseActorForMatching(entry) === normalised) return true;
  }
  return false;
}

export function inferredMutationActor(): string | null {
  return (
    trimToNull(process.env.OPENCLAW_AGENT ? `agent:${process.env.OPENCLAW_AGENT}` : null) ??
    trimToNull(process.env.RENTEMESTER_AGENT ? `agent:${process.env.RENTEMESTER_AGENT}` : null) ??
    trimToNull(process.env.RENTEMESTER_USER ? `user:${process.env.RENTEMESTER_USER}` : null) ??
    trimToNull(process.env.USER ? `user:${process.env.USER}` : null) ??
    trimToNull(process.env.LOGNAME ? `user:${process.env.LOGNAME}` : null) ??
    // Windows has no USER/LOGNAME; the logged-in account name is in USERNAME.
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

/** Outcome of the shared allowlist gate. */
export type ActorAllowlistDecision =
  | { allowed: true }
  | { allowed: false; reason: string };

/**
 * SEC-2 / SEC-3 (Audit 2026-06-11): the transport-agnostic CORE of the actor
 * allowlist gate. The CLI (`enforceMutationActorPolicy`) and the MCP write
 * path (`withCompanyDbConfirmed`) BOTH call this, so a confirmed write is held
 * to the same allowlist no matter which surface issued it. Previously the
 * allowlist was enforced only in the CLI, so an agent over MCP could perform
 * any confirmed write regardless of the policy.
 *
 * The caller is responsible for having already validated the actor's canonical
 * format (`isCanonicalActorId`); this function only answers "is this actor in
 * the policy?".
 *
 * SEC-3 fail-closed: an EMPTY allowlist means there is no `config/policy.yaml`
 * to enforce against (uninitialised path, or a deleted/absent policy). The old
 * behaviour accepted ANY actor in that case — fail-OPEN — which let an explicit
 * `--actor user:<human>` through against a company that has no policy at all.
 *
 * We now fail closed for `user:` actors specifically: a human identity asserts
 * personal authorship into the append-only audit trail, and accepting it
 * against a company that has NO governing policy is exactly the hole the audit
 * flagged. Onboarding (`init` / `company add`) always seeds the allowlist (see
 * `buildDefaultPolicyYaml`), so a real, initialised single-user company always
 * has a non-empty allowlist and is unaffected — only a policy-less company
 * rejects a `user:` actor here.
 *
 * `agent:` / `system:` actors are still allowed through an empty allowlist:
 * those are deliberate machine identities used during bootstrap (and the
 * build-phase default for the MCP transport, where no per-company policy may
 * exist yet). Once a policy DOES exist, every actor — including agents — is
 * matched against it.
 */
export function checkActorAllowlist(root: string, actor: string): ActorAllowlistDecision {
  const allowlist = loadActorAllowlist(root);
  if (allowlist.size === 0) {
    const [kind] = actor.split(":", 1);
    if (kind === "user") {
      return {
        allowed: false,
        reason:
          `no actor_allowlist found in config/policy.yaml for this company — ` +
          `refusing user actor '${actor}' (fail-closed). Run onboarding (\`init\` / ` +
          `\`company add\`) so the allowlist is seeded, then add '${actor}' if needed.`,
      };
    }
    return { allowed: true };
  }
  if (!actorMatchesAllowlist(actor, allowlist)) {
    return {
      allowed: false,
      reason:
        `actor '${actor}' is not in config/policy.yaml actor_allowlist. ` +
        howToAddActorHint(actor),
    };
  }
  return { allowed: true };
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
    const decision = checkActorAllowlist(root, explicitActor);
    if (!decision.allowed) fatal(decision.reason);
    process.env.RENTEMESTER_ACTOR = explicitActor;
    if (cliActorVia) process.env.RENTEMESTER_ACTOR_VIA = cliActorVia;
    else if (!trimToNull(process.env.RENTEMESTER_ACTOR_VIA))
      process.env.RENTEMESTER_ACTOR_VIA = "rentemester-cli";
    return;
  }
  // No explicit --actor: the entry is attributed to a derived actor (OS
  // username / agent env var). #248: the allowlist is consistent — the
  // derived path is held to the SAME rule as an explicit `--actor`. The
  // person who runs onboarding (`init` / `company add`) is seeded into the
  // allowlist automatically, so on the happy path no friction is added. An
  // un-seeded derived actor now gets the same clear hint as an unseeded
  // explicit one, instead of silently slipping through and writing an actor
  // to the audit trail that the same rule would have rejected if stated
  // explicitly.
  const derivedActor = inferredMutationActor();
  if (!derivedActor) {
    fatal(
      "actor required for mutations: pass --actor <user:...|agent:...|system:...> or run with USER/LOGNAME/USERNAME/OPENCLAW_AGENT set",
    );
  }
  // #283 / SEC-3: `system restore-backup` recreates a company from a backup,
  // so its `--target-company` is normally brand new and cannot yet hold a
  // `config/policy.yaml`. The explicit-actor branch above already carves this
  // out; the derived path needs the identical carve-out, otherwise a restore
  // without `--actor` fail-closes against the absent target policy (the SEC-3
  // tightening would reject a derived `user:` actor here). A restore into an
  // EXISTING company (which does have a policy) is still fully enforced below.
  if (
    commandKey === "system restore-backup" &&
    !existsSync(join(companyPaths(root).config, "policy.yaml"))
  ) {
    return;
  }
  // SEC-3 (Audit 2026-06-11): the derived path is held to the SAME shared gate
  // as the explicit path, INCLUDING the fail-closed empty-allowlist rule. An
  // un-initialised company (no `config/policy.yaml`) used to fail OPEN here —
  // any derived OS username silently passed. Now a derived actor against an
  // absent policy is rejected, exactly like an explicit one.
  const decision = checkActorAllowlist(root, derivedActor);
  if (!decision.allowed) fatal(decision.reason);
}
