const BOOLEAN_FLAGS = new Set(["--force", "--help", "--example", "--json", "--sign-with-ed25519"]);

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

  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i]!;
    if (!token.startsWith("--")) {
      positionals.push(token);
      continue;
    }

    if (BOOLEAN_FLAGS.has(token)) {
      const next = tokens[i + 1];
      if (next && !next.startsWith("--")) {
        errors.push(`Flag ${token} does not take a value`);
        i += 1;
        continue;
      }
      flags.set(token, true);
      continue;
    }

    const value = tokens[i + 1];
    if (!value || value.startsWith("--")) {
      errors.push(`Flag ${token} requires a value`);
      continue;
    }
    flags.set(token, value);
    i += 1;
  }

  return { positionals, flags, errors };
}
