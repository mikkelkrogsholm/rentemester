import type { ParsedCliArgs } from "./cli-args";
import type { OutputFormat } from "./cli-format";
import { printStructuredResult } from "./cli-format";
import { getCommandSpec } from "./cli-meta";

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
