const BOOLEAN_FLAGS = new Set([
  "--force",
  "--help",
  "--example",
  "--json",
  "--sign-with-ed25519",
  "--archive",
  "--enrich-cvr",
  "--include-archived",
  "--ixbrl-taxonomy",
  "--after-retention-expiry",
]);

export type ParsedCliArgs = {
  positionals: string[];
  flags: Map<string, string | true>;
  errors: string[];
};

export function parseCliArgs(argv: string[]): ParsedCliArgs {
  const tokens = argv.slice(2);
  const positionals: string[] = [];
  const flags = new Map<string, string | true>();
  const errors: string[] = [];

  let optionsTerminated = false;

  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i]!;

    if (!optionsTerminated && token === "--") {
      optionsTerminated = true;
      continue;
    }

    if (optionsTerminated || !token.startsWith("--")) {
      positionals.push(token);
      continue;
    }

    // `--flag=value` form: value is attached, so it may begin with dashes
    // or be empty without being confused for a missing-value error.
    const eq = token.indexOf("=");
    if (eq !== -1) {
      const name = token.slice(0, eq);
      const value = token.slice(eq + 1);
      if (BOOLEAN_FLAGS.has(name)) {
        errors.push(`Flag ${name} does not take a value`);
        continue;
      }
      flags.set(name, value);
      continue;
    }

    if (BOOLEAN_FLAGS.has(token)) {
      const next = tokens[i + 1];
      if (next && next !== "--" && !next.startsWith("--")) {
        errors.push(`Flag ${token} does not take a value`);
        i += 1;
        continue;
      }
      flags.set(token, true);
      continue;
    }

    const value = tokens[i + 1];
    if (value === undefined || value === "--" || value.startsWith("--")) {
      errors.push(`Flag ${token} requires a value`);
      continue;
    }
    flags.set(token, value);
    i += 1;
  }

  return { positionals, flags, errors };
}
