#!/usr/bin/env bun
import { existsSync, readFileSync } from "node:fs";
import { isAbsolute, resolve, sep } from "node:path";
import { parseCliArgs } from "./cli-args";
import { resolveOutputFormat } from "./cli-format";
import {
  getCommandSpec,
  renderCommandHelp,
  renderGlobalUsage,
  validateCommandFlags,
} from "./cli-meta";
import {
  CommandDispatch,
  createEmitResult,
  resolveCommandLabel,
  type CommandContext,
} from "./cli-dispatch";
import {
  MUTATING_COMMANDS,
  enforceMutationActorPolicy,
  inferredMutationActor,
  trimToNull,
} from "./cli-actor";
import { register as registerInit } from "./cli/init";
import { register as registerAudit } from "./cli/audit";
import { register as registerAccounts } from "./cli/accounts";
import { register as registerExceptions } from "./cli/exceptions";
import { register as registerInvoice } from "./cli/invoice";
import { register as registerDocuments } from "./cli/documents";
import { register as registerBank } from "./cli/bank";
// ===== BANK CLUSTER (#187) =====
import { register as registerBankAccount } from "./cli/bank-account";
// ===== END BANK CLUSTER (#187) =====
import { register as registerVat } from "./cli/vat";
import { register as registerJournal } from "./cli/journal";
import { register as registerSystem } from "./cli/system";
import { register as registerCustomer } from "./cli/customer";
import { register as registerVendor } from "./cli/vendor";
import { register as registerExpense } from "./cli/expense";
import { register as registerRetention } from "./cli/retention";
import { register as registerPeriod } from "./cli/period";
import { register as registerDashboard } from "./cli/dashboard";
import { register as registerCompliance } from "./cli/compliance";
// ===== RECURRING INVOICES (#118) =====
import { register as registerRecurringInvoice } from "./cli/recurring-invoice";
// ===== END RECURRING INVOICES (#118) =====
// ===== MAIL INTAKE (#122) =====
import { register as registerMailIntake } from "./cli/mail-intake";
// ===== IMAP INTAKE (#181) =====
import { register as registerImapIntake } from "./cli/imap-intake";
// ===== END IMAP INTAKE (#181) =====
// ===== MILEAGE LOG (#123) =====
import { register as registerMileage } from "./cli/mileage";
// Fixed assets (#124, #125)
import { register as registerAsset } from "./cli/asset";
import { register as registerCompany } from "./cli/company";
// ===== COCKPIT BACKEND (#170) =====
import { register as registerServe } from "./cli/serve";
// ===== FINANCIAL STATEMENTS (#176) =====
import { register as registerReport } from "./cli/report";
// ===== END FINANCIAL STATEMENTS (#176) =====
// ===== OPENING BALANCE (#179) =====
import { register as registerOpeningBalance } from "./cli/opening-balance";
// ===== END OPENING BALANCE (#179) =====
// ===== EMAIL DELIVERY (#180) =====
import { register as registerEmail } from "./cli/email";
// ===== END EMAIL DELIVERY (#180) =====
// ===== GDPR (#184) =====
import { register as registerGdpr } from "./cli/gdpr";
// ===== END GDPR (#184) =====
// ===== ANNUAL REPORT (#177) =====
import { register as registerAnnualReport } from "./cli/annual-report";
// ===== END ANNUAL REPORT (#177) =====
// ===== IMPORT FRAMEWORK (#185) =====
import { register as registerImport } from "./cli/import";
// ===== END IMPORT FRAMEWORK (#185) =====
// ===== RUNTIME AGENT (#183) =====
import { register as registerAgent } from "./cli/agent";
// ===== END RUNTIME AGENT (#183) =====
// ===== REGULATORY COVERAGE =====
import { register as registerReg } from "./cli/reg";
// ===== END REGULATORY COVERAGE =====
// ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
import { register as registerAccrual } from "./cli/accrual";
// ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
// ===== TAX RETURN PREPARATION =====
import { register as registerTax } from "./cli/tax";
// ===== END TAX RETURN PREPARATION =====
// ===== BUDGET + LIQUIDITY FORECAST =====
import { register as registerBudget } from "./cli/budget";
// ===== END BUDGET + LIQUIDITY FORECAST =====
// ===== PAYABLES / KREDITORSTYRING =====
import { register as registerPayable } from "./cli/payable";
// ===== END PAYABLES / KREDITORSTYRING =====
import {
  isValidSlug,
  resolveConfiguredWorkspaceRoot,
  resolveWorkspaceSlug,
} from "./core/workspace";
import { migrate, openDb } from "./core/db";
import { companyPaths } from "./core/paths";
import { evaluateBackupLock } from "./core/backup-governance";

function fatal(message: string): never {
  console.error(message);
  process.exit(2);
}

/**
 * Resolves the company root from `--company` / `RENTEMESTER_COMPANY`.
 *
 * `--company` accepts EITHER a workspace slug OR a raw path:
 *  - A bare slug (lowercase letters/digits/dashes, no path separator) is
 *    resolved against the configured `RENTEMESTER_WORKSPACE`. If a workspace
 *    is configured and the slug is registered, its company directory is used.
 *    If a workspace is configured but the slug is unknown, the command fails
 *    with a clear error.
 *  - Anything else (or a bare slug with no workspace configured) is treated as
 *    a raw path — the original, unchanged behaviour for tests/smoke/Docker.
 *
 * There is no silent default: a command that needs a company but was given
 * none fails with a clear error. A raw path is rejected if it contains
 * parent-directory (`..`) segments, then `resolve()`d to an absolute path so
 * the ledger/backup tree cannot be relocated by a traversal payload.
 */
function resolveCompanyRoot(): string {
  const raw =
    trimToNull(parsedArgs.flags.get("--company") as string | undefined) ??
    trimToNull(process.env.RENTEMESTER_COMPANY);
  if (!raw) {
    fatal(
      "--company is required: pass --company <slug|path> or set RENTEMESTER_COMPANY",
    );
  }

  // Slug resolution: only a bare, separator-free, slug-shaped value is a
  // candidate, so a real path can never be misread as a slug.
  const looksLikeBareSlug = !raw.includes("/") && !raw.includes("\\") && isValidSlug(raw);
  if (looksLikeBareSlug) {
    let workspaceRoot: string | null;
    try {
      workspaceRoot = resolveConfiguredWorkspaceRoot();
    } catch (error) {
      fatal(error instanceof Error ? error.message : String(error));
    }
    if (workspaceRoot) {
      const fromSlug = resolveWorkspaceSlug(workspaceRoot, raw);
      if (fromSlug) return fromSlug;
      fatal(
        `--company '${raw}': no company with that slug in workspace ${workspaceRoot}. ` +
          `Run 'rentemester company list' or pass a path instead.`,
      );
    }
  }

  const segments = raw.split(/[\\/]+/);
  if (segments.includes("..")) {
    fatal("--company must not contain parent-directory ('..') segments");
  }
  const resolved = resolve(raw);
  if (!isAbsolute(resolved) || resolved.split(sep).includes("..")) {
    fatal("--company resolved to an unsafe path");
  }
  return resolved;
}

const parsedArgs = parseCliArgs(Bun.argv);
const [cmd, sub] = parsedArgs.positionals;
const commandSpec = getCommandSpec(cmd, sub);
const outputFormat = resolveOutputFormat(parsedArgs.flags);
const commandKey = [cmd, sub].filter(Boolean).join(" ");
const cliActor = trimToNull(parsedArgs.flags.get("--actor") as string | undefined);
const cliActorVia = trimToNull(parsedArgs.flags.get("--actor-via") as string | undefined);

if (parsedArgs.errors.length > 0) fatal(parsedArgs.errors.join("\n"));
if (outputFormat === null) fatal("--format must be either json or human");
const flagErrors = validateCommandFlags(cmd, sub, parsedArgs.flags.keys());
if (flagErrors.length > 0) fatal(flagErrors.join("\n"));
if (parsedArgs.flags.has("--example")) {
  if (!commandSpec?.examplePath)
    fatal(`No example is registered for ${cmd}${sub ? ` ${sub}` : ""}`);
  process.stdout.write(readFileSync(commandSpec.examplePath, "utf8"));
  process.exit(0);
}

const ctx: CommandContext = {
  parsedArgs,
  outputFormat: outputFormat!,
  commandKey,
  cmd: cmd ?? "",
  sub,
  arg(name, fallback) {
    const value = parsedArgs.flags.get(name);
    return typeof value === "string" ? value : fallback;
  },
  hasFlag(name) {
    return parsedArgs.flags.has(name);
  },
  companyRoot() {
    return resolveCompanyRoot();
  },
  cliActor,
  cliActorVia,
  inferredMutationActor,
  fatal,
  emitResult: createEmitResult(outputFormat!, resolveCommandLabel(cmd, sub)),
  trimToNull,
  parseOptionalNumber(flagName) {
    const value = this.arg(flagName);
    if (value === undefined) return { ok: true as const, value: undefined };
    const parsed = Number(value);
    if (Number.isNaN(parsed))
      return { ok: false as const, error: `${flagName} must be numeric when present` };
    return { ok: true as const, value: parsed };
  },
};

const dispatch = new CommandDispatch();
for (const registerFn of [
  registerInit,
  registerAudit,
  registerAccounts,
  registerExceptions,
  registerInvoice,
  registerDocuments,
  registerBank,
  // ===== BANK CLUSTER (#187) =====
  registerBankAccount,
  // ===== END BANK CLUSTER (#187) =====
  registerVat,
  registerJournal,
  registerSystem,
  registerCustomer,
  registerVendor,
  registerExpense,
  registerRetention,
  registerPeriod,
  registerDashboard,
  // ===== RECURRING INVOICES (#118) =====
  registerRecurringInvoice,
  // ===== END RECURRING INVOICES (#118) =====
  // ===== MAIL INTAKE (#122) =====
  registerMailIntake,
  // ===== IMAP INTAKE (#181) =====
  registerImapIntake,
  // ===== END IMAP INTAKE (#181) =====
  // ===== MILEAGE LOG (#123) =====
  registerMileage,
  // Fixed assets (#124, #125)
  registerAsset,
  registerCompany,
  // ===== COCKPIT BACKEND (#170) =====
  registerServe,
  // ===== FINANCIAL STATEMENTS (#176) =====
  registerReport,
  // ===== END FINANCIAL STATEMENTS (#176) =====
  // ===== OPENING BALANCE (#179) =====
  registerOpeningBalance,
  // ===== END OPENING BALANCE (#179) =====
  // ===== EMAIL DELIVERY (#180) =====
  registerEmail,
  // ===== END EMAIL DELIVERY (#180) =====
  // ===== GDPR (#184) =====
  registerGdpr,
  // ===== END GDPR (#184) =====
  // ===== ANNUAL REPORT (#177) =====
  registerAnnualReport,
  // ===== END ANNUAL REPORT (#177) =====
  // ===== IMPORT FRAMEWORK (#185) =====
  registerImport,
  // ===== END IMPORT FRAMEWORK (#185) =====
  // ===== RUNTIME AGENT (#183) =====
  registerAgent,
  // ===== END RUNTIME AGENT (#183) =====
  // ===== REGULATORY COVERAGE =====
  registerReg,
  registerCompliance,
  // ===== END REGULATORY COVERAGE =====
  // ===== TAX RETURN PREPARATION =====
  registerTax,
  // ===== END TAX RETURN PREPARATION =====
  // ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
  registerAccrual,
  // ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
  // ===== BUDGET + LIQUIDITY FORECAST =====
  registerBudget,
  // ===== END BUDGET + LIQUIDITY FORECAST =====
  // ===== PAYABLES / KREDITORSTYRING =====
  registerPayable,
  // ===== END PAYABLES / KREDITORSTYRING =====
]) {
  registerFn(dispatch);
}

if (!cmd || cmd === "help") {
  console.log(renderGlobalUsage());
} else if (parsedArgs.flags.has("--help")) {
  if (commandSpec) console.log(renderCommandHelp(commandSpec));
  else console.log(renderGlobalUsage());
} else {
  const handler = dispatch.get(cmd, sub);
  if (!handler) {
    console.error(`Unknown command: ${cmd}${sub ? " " + sub : ""}`);
    console.log(renderGlobalUsage());
    process.exit(2);
  }
  // Enforce the actor policy only when actually executing a mutating
  // command — never for `help` / `--help`, which neither read nor write
  // company data. `restore-backup` writes to --target-company, not
  // --company, so resolve its policy root from that flag.
  if (MUTATING_COMMANDS.has(commandKey)) {
    const mutationRoot = commandKey === "system restore-backup"
      ? trimToNull(parsedArgs.flags.get("--target-company") as string | undefined)
      : ctx.companyRoot();
    if (mutationRoot) {
      enforceMutationActorPolicy(commandKey, mutationRoot, cliActor, cliActorVia, fatal);
    }
    // Opt-in backup lock: when the owner has enabled enforcement and a
    // weekly backup (BEK 205/2024 §4) is overdue past the grace window,
    // bookkeeping mutations are refused. `system …` commands are exempt by
    // design — backing up and restoring must always stay possible, since
    // that is the only way out of the lock.
    //
    // A lock rejection is a *business rejection*, not a usage error: the
    // call was well-formed, but a precondition (a fresh backup) is missing.
    // Per `docs/cli-contract.md` that is exit 1 (`ok:false` envelope), not
    // exit 2 — so we emit the same `{ ok:false, errors:[...] }` result an
    // agent gets from a ledger rejection and let `emitResult` set exit 1.
    // (#258)
    if (mutationRoot && !commandKey.startsWith("system ") && existsSync(companyPaths(mutationRoot).db)) {
      const lockDb = openDb(companyPaths(mutationRoot).db);
      let lockReason: string | null = null;
      try {
        migrate(lockDb);
        const lock = evaluateBackupLock(lockDb, mutationRoot);
        if (lock.locked) lockReason = lock.reason;
      } finally {
        lockDb.close();
      }
      if (lockReason !== null) {
        ctx.emitResult({
          ok: false,
          errors: [
            `Bogføring er låst (${commandKey}): ${lockReason}. ` +
              "Kør 'rentemester system backup' for at låse op. Placér derefter backup-arkivet " +
              "på en EU/EØS-destination med 'rentemester system backup-place' for at opfylde " +
              "BEK 205/2024 § 4. Se status med 'rentemester system backup-governance'.",
          ],
          backupLock: { locked: true, command: commandKey },
        });
        process.exit(process.exitCode ?? 1);
      }
    }
  }
  await handler(ctx);
}
