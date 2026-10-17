import { supportedSchemaMigrations } from "../../src/core/schema-version";

// Exact source artifacts are authoritative; this is not a new migration catalogue.
const destination = new URL(
  "../../crates/rentemester-storage/src/legacy-schema.json",
  import.meta.url,
);
const expected = `${JSON.stringify(supportedSchemaMigrations(), null, 2)}\n`;
if (process.argv.includes("--write")) {
  await Bun.write(destination, expected);
} else if ((await Bun.file(destination).text()) !== expected) {
  throw new Error(
    "Rust legacy-schema.json drifted from exact TypeScript migration artifacts; review compatibility before regenerating",
  );
}
