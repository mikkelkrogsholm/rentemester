import { MUTATING_COMMANDS } from "../cli-actor";
import { SIDE_EFFECTING_COMMANDS, flagsForSpec, type CommandSpec } from "./_shared";

/**
 * The helpers operate on the assembled `COMMAND_SPECS` array. The barrel
 * (`src/cli-meta.ts`) builds that array from the per-domain spec files and
 * then calls `registerCommandSpecs(...)` exactly once at import time. We use a
 * register-callback rather than importing the barrel directly so there is no
 * `helpers <-> barrel` runtime cycle.
 */
let COMMAND_SPECS: readonly CommandSpec[] = [];
let SPEC_MAP: Map<string, CommandSpec> = new Map();

export function registerCommandSpecs(specs: readonly CommandSpec[]): void {
  COMMAND_SPECS = specs;
  SPEC_MAP = new Map(specs.map((spec) => [spec.key, spec]));
}

export function getCommandKey(cmd?: string, sub?: string) {
  if (!cmd) return "";
  return sub ? `${cmd} ${sub}` : cmd;
}

export function getCommandSpec(cmd?: string, sub?: string) {
  return SPEC_MAP.get(getCommandKey(cmd, sub));
}

export function renderGlobalUsage() {
  const lines = [
    "Rentemester v0.0.1 — agent-først dansk bogholderi",
    "",
    "Brug:  rentemester <kommando> [underkommando] [flags]",
    "       rentemester <kommando> --help     # detaljeret hjælp + inputnoter for én kommando",
    "",
    "Læsekommandoer (read-only — ingen sideeffekter, kræver ingen actor):",
  ];
  // Three groups, not two: MUTATING_COMMANDS is the single source of truth for
  // actor-gated ledger mutations (#231), but a command can write files/records
  // without being an actor-gated ledger mutation — `init`, `company add` and
  // `import contacts` do exactly that. Labelling those "read-only" invites an
  // agent to call them speculatively, so they get their own heading. (#239)
  for (const spec of COMMAND_SPECS) {
    if (MUTATING_COMMANDS.has(spec.key) || SIDE_EFFECTING_COMMANDS.has(spec.key)) continue;
    lines.push(`  ${spec.key.padEnd(34)} ${firstSentence(spec.description)}`);
  }
  lines.push(
    "",
    "Opsætningskommandoer (opretter mapper/databaser/poster — ingen actor, men ikke read-only):",
  );
  for (const spec of COMMAND_SPECS) {
    if (!SIDE_EFFECTING_COMMANDS.has(spec.key)) continue;
    lines.push(`  ${spec.key.padEnd(34)} ${firstSentence(spec.description)}`);
  }
  lines.push("", "Skrivekommandoer (muterer bogføringen — kræver en actor, se nedenfor):");
  for (const spec of COMMAND_SPECS) {
    if (!MUTATING_COMMANDS.has(spec.key)) continue;
    lines.push(`  ${spec.key.padEnd(34)} ${firstSentence(spec.description)}`);
  }
  lines.push(
    "",
    "Globale flags:",
    "  --help                 Viser hjælp for kommandoen (eller denne oversigt).",
    "  --example              Skriver et eksempel-input til stdout (kun kommandoer med eksempel).",
    "  --format json|human    Vælger outputformat (standard: human i terminal, json ellers).",
    "  --json                 Genvej for --format json.",
    "  --actor <id>           Den ansvarlige aktør for en muterende kommando. Format:",
    "                         user:<navn> | agent:<navn> | system:<navn>. Skal stå i",
    "                         config/policy.yaml (actor_allowlist). Uden actor afvises",
    "                         enhver skrivekommando med 'actor required for mutations'.",
    "  --actor-via <kanal>    Valgfri: hvordan handlingen blev udløst (fx cli, cockpit, mcp).",
    "",
    "Exit-koder: 0 = ok · 2 = parse-/brugsfejl (ret kaldet) · 1 = forretningsafvisning (læs errors[]).",
  );
  return lines.join("\n");
}

/** First sentence of a Danish description, for the compact command index. */
function firstSentence(text: string): string {
  const idx = text.indexOf(". ");
  const first = idx >= 0 ? text.slice(0, idx + 1) : text;
  return first.length > 90 ? first.slice(0, 89) + "…" : first;
}

export function renderCommandHelp(spec: CommandSpec) {
  const lines = [spec.description, "", "Brug:", `  rentemester ${spec.usage}`];
  if (spec.inputNotes?.length) {
    lines.push("", "Inputnoter:");
    for (const note of spec.inputNotes) lines.push(`  - ${note}`);
  }
  if (spec.examplePath) {
    lines.push("", "Eksempel:", `  ${spec.exampleHint ?? `rentemester ${spec.key} --example`}`, `  # Kilde: ${spec.examplePath}`);
    if (spec.exampleNote) lines.push(`  # ${spec.exampleNote}`);
  }
  lines.push("", "Tilladte flags:");
  for (const flag of [...spec.allowedFlags, ...flagsForSpec(spec)]) lines.push(`  ${flag}`);
  return lines.join("\n");
}

export function validateCommandFlags(cmd: string | undefined, sub: string | undefined, flags: Iterable<string>) {
  const spec = getCommandSpec(cmd, sub);
  if (!spec) return [] as string[];
  const allowed = new Set([...spec.allowedFlags, ...flagsForSpec(spec)]);
  const errors: string[] = [];
  for (const flag of flags) {
    if (allowed.has(flag)) continue;
    const suggestion = suggestFlag(flag, [...allowed]);
    errors.push(`Unknown flag ${flag} for ${spec.key}.${suggestion ? ` Did you mean ${suggestion}?` : ""}`);
  }
  return errors;
}

function suggestFlag(input: string, candidates: string[]) {
  let best: { value: string; score: number } | null = null;
  for (const candidate of candidates) {
    const score = levenshtein(input, candidate);
    if (score > 3) continue;
    if (!best || score < best.score) best = { value: candidate, score };
  }
  return best?.value ?? null;
}

function levenshtein(a: string, b: string) {
  const dp = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = 0; i <= a.length; i += 1) dp[i]![0] = i;
  for (let j = 0; j <= b.length; j += 1) dp[0]![j] = j;
  for (let i = 1; i <= a.length; i += 1) {
    for (let j = 1; j <= b.length; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      dp[i]![j] = Math.min(dp[i - 1]![j]! + 1, dp[i]![j - 1]! + 1, dp[i - 1]![j - 1]! + cost);
    }
  }
  return dp[a.length]![b.length]!;
}
