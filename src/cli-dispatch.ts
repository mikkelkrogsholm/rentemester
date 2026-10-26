import type { Database } from "bun:sqlite";
import { readFileSync } from "node:fs";
import type { ParsedCliArgs } from "./cli-args";
import type { OutputFormat } from "./cli-format";
import { printStructuredResult } from "./cli-format";
import { getCommandSpec } from "./cli-meta";
import { SIDE_EFFECTING_COMMANDS } from "./cli-meta";
import { MUTATING_COMMANDS } from "./cli-actor";
import { openDb } from "./core/db";
import { openCurrentLedgerReadOnly } from "./core/ledger-inspection";
import { companyPaths } from "./core/paths";

export type CommandContext = {
  parsedArgs: ParsedCliArgs;
  outputFormat: OutputFormat;
  commandKey: string;
  cmd: string;
  sub: string | undefined;
  arg(name: string, fallback?: string): string | undefined;
  hasFlag(name: string): boolean;
  companyRoot(): string;
  cliActor: string | null;
  cliActorVia: string | null;
  inferredMutationActor(): string | null;
  fatal(message: string): never;
  emitResult(result: Record<string, unknown>, commandLabel?: string): void;
  trimToNull(value: string | null | undefined): string | null;
  parseOptionalNumber(flagName: string):
    | { ok: true; value: number | undefined }
    | { ok: false; error: string };
};

export type CommandHandler = (ctx: CommandContext) => void | Promise<void>;

/**
 * Åbner company-databasen for et CLI-command-ctx. Erstatter den copy-paste'ede
 * `openDb(companyPaths(ctx.companyRoot()).db)`-linje i CLI-handlerne.
 * Read-only commands receive an isolated, current-schema snapshot. Mutations
 * and explicit setup commands receive the writable handle and remain
 * responsible for `migrate(db)`. Existing read handlers may still call
 * `migrate(db)`; on a query-only snapshot it is a validation-only no-op.
 */
export function openCommandDb(ctx: CommandContext): Database {
  const dbPath = companyPaths(ctx.companyRoot()).db;
  const writable = MUTATING_COMMANDS.has(ctx.commandKey) || SIDE_EFFECTING_COMMANDS.has(ctx.commandKey);
  return writable ? openDb(dbPath) : openCurrentLedgerReadOnly(dbPath);
}

/** Resolve the CLI's discriminated numeric parser at one trusted boundary. */
export function optionalNumberOrFatal(
  ctx: CommandContext,
  flagName: string,
): number | undefined {
  const parsed = ctx.parseOptionalNumber(flagName);
  if (!parsed.ok) ctx.fatal(parsed.error);
  return parsed.value;
}

export function requiredNumberOrFatal(
  ctx: CommandContext,
  flagName: string,
): number {
  const value = optionalNumberOrFatal(ctx, flagName);
  if (value === undefined) ctx.fatal(`Missing required ${flagName} <n>`);
  return value;
}

/**
 * Læs + parse en JSON-input-fil for et CLI-kald. Erstatter den copy-paste'ede
 * `JSON.parse(readFileSync(input, "utf8"))`-linje i CLI-handlerne, som
 * tidligere kastede rå Node-`ENOENT`-stack traces (eller `SyntaxError`) på
 * almindelige bruger-fejl: glemt sti, forkert sti, ikke-JSON fil. Round-2
 * review flagede dette som "ligner softwarefejl, ikke brugerfejl".
 *
 * Funktionen kalder selv `ctx.fatal(...)` (exit 2 = parse-fejl) med en kort,
 * dansk fejl-besked og returnerer aldrig på fejl — så callers kan bruge den
 * synkront uden try/catch:
 *
 *     const payload = readJsonCliInput(ctx, input, "--input");
 *     postJournalEntry(db, payload);
 */
export function readJsonCliInput(
  ctx: CommandContext,
  path: string,
  flagName: string,
): unknown {
  let raw: string;
  try {
    raw = readFileSync(path, "utf8");
  } catch (err) {
    const code = (err as NodeJS.ErrnoException | undefined)?.code;
    if (code === "ENOENT") {
      ctx.fatal(`filen findes ikke: '${path}' — angiv en eksisterende fil med ${flagName}`);
    }
    if (code === "EACCES" || code === "EPERM") {
      ctx.fatal(`ingen læseadgang til '${path}' (${code}) — tjek fil-rettigheder eller kør med en bruger der må læse filen`);
    }
    if (code === "EISDIR") {
      ctx.fatal(`'${path}' er en mappe, ikke en fil — ${flagName} skal pege på en konkret .json-fil`);
    }
    const detail = err instanceof Error ? err.message : String(err);
    ctx.fatal(`kunne ikke læse '${path}': ${detail}`);
  }
  try {
    return JSON.parse(raw);
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    ctx.fatal(`'${path}' er ikke gyldig JSON (${detail}) — tjek for kommafejl, manglende parenteser, eller om filen er gemt i et andet format`);
  }
}

/** JSON object boundary for domain commands; arrays and primitives are usage errors. */
export function readJsonObjectCliInput(
  ctx: CommandContext,
  filePath: string,
  flagName: string,
): Record<string, unknown> {
  const value = readJsonCliInput(ctx, filePath, flagName);
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    ctx.fatal(`${flagName} must contain a JSON object`);
  }
  return value as Record<string, unknown>;
}

export class CommandDispatch {
  private handlers = new Map<string, CommandHandler>();

  on(cmd: string, sub: string | null, handler: CommandHandler): void {
    const key = sub ? `${cmd} ${sub}` : cmd;
    if (this.handlers.has(key)) {
      throw new Error(`Duplicate command handler registered for: ${key}`);
    }
    this.handlers.set(key, handler);
  }

  get(cmd: string | undefined, sub: string | undefined): CommandHandler | undefined {
    if (!cmd) return undefined;
    const key = sub ? `${cmd} ${sub}` : cmd;
    return this.handlers.get(key);
  }

  has(cmd: string | undefined, sub: string | undefined): boolean {
    return this.get(cmd, sub) !== undefined;
  }
}

export function createEmitResult(
  outputFormat: OutputFormat,
  fallbackCommandLabel: string,
) {
  return function emitResult(
    result: Record<string, unknown>,
    commandLabel?: string,
  ): void {
    const label = commandLabel ?? fallbackCommandLabel;
    printStructuredResult(label, result, outputFormat);
    if (result.ok === false) process.exitCode = 1;
  };
}

export function resolveCommandLabel(cmd: string | undefined, sub: string | undefined) {
  const spec = getCommandSpec(cmd, sub);
  if (spec) return spec.description;
  return [cmd, sub].filter(Boolean).join(" ");
}
