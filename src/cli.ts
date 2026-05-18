#!/usr/bin/env bun
import { readFileSync } from "node:fs";
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
import { register as registerVat } from "./cli/vat";
import { register as registerJournal } from "./cli/journal";
import { register as registerSystem } from "./cli/system";
import { register as registerCustomer } from "./cli/customer";
import { register as registerVendor } from "./cli/vendor";
import { register as registerExpense } from "./cli/expense";
import { register as registerRetention } from "./cli/retention";
import { register as registerPeriod } from "./cli/period";

function fatal(message: string): never {
  console.error(message);
  process.exit(2);
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
    return this.arg("--company", process.env.RENTEMESTER_COMPANY ?? "/company")!;
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
  registerVat,
  registerJournal,
  registerSystem,
  registerCustomer,
  registerVendor,
  registerExpense,
  registerRetention,
  registerPeriod,
]) {
  registerFn(dispatch);
}

enforceMutationActorPolicy(commandKey, ctx.companyRoot(), cliActor, cliActorVia, fatal);

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
  await handler(ctx);
}
