export type CommandSpec = {
  key: string;
  usage: string;
  description: string;
  allowedFlags: string[];
  examplePath?: string;
  exampleHint?: string;
  /**
   * One-line clarification of what `--example` actually prints, shown under
   * the "Eksempel" block in `--help`. Use it when the example file is NOT a
   * complete call's input but a *fragment* — e.g. a `--metadata` payload that
   * still has to be combined with `--company`/`--file` flags. Without this an
   * agent may pipe the example straight in as if it were the whole input.
   */
  exampleNote?: string;
  inputNotes?: string[];
};

export const GLOBAL_FLAGS = ["--help", "--example", "--format", "--json", "--actor", "--actor-via"];

/**
 * Global flags valid for every command regardless of spec. `--example` is
 * deliberately excluded: it only works for commands that registered an
 * `examplePath`, so it is added per-command in `flagsForSpec`. Advertising it
 * everywhere let callers pass `--example` to a command with no example, which
 * then failed with exit 2. (#244)
 */
export const UNIVERSAL_GLOBAL_FLAGS = GLOBAL_FLAGS.filter((flag) => flag !== "--example");

/** The global flags a given command actually accepts (adds `--example` only when it has one). */
export function flagsForSpec(spec: CommandSpec): string[] {
  return spec.examplePath
    ? [...UNIVERSAL_GLOBAL_FLAGS, "--example"]
    : UNIVERSAL_GLOBAL_FLAGS;
}

/**
 * Commands that write files or records but are NOT actor-gated ledger
 * mutations — so they fall outside `MUTATING_COMMANDS`, yet are emphatically
 * not read-only. `init`/`company add` create company directories, ledger
 * databases and the workspace manifest; `import contacts` writes customer and
 * vendor records; `company set-profile` rewrites the company's own master
 * data and appends an audit-log entry (#267). The global help must not invite
 * an agent to call these speculatively under a "read-only" heading. (#239)
 */
export const SIDE_EFFECTING_COMMANDS = new Set([
  "init",
  "company add",
  "company set-profile",
  "import contacts",
]);
