import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

export type MigrationArtifact = { id: number; name: string; sql?: string };
export type LoadedMigration = MigrationArtifact & { checksum: string; artifact: Buffer };

/** Loads immutable migration artifacts, validating their filename and identity. */
export function loadMigrationCatalog(directory: string, legacyNames: Readonly<Record<number, string>> = {}, excludedFiles: readonly string[] = []): readonly LoadedMigration[] {
  const migrations = readdirSync(directory)
    .filter((file) => /^\d{4}-.+\.json$/.test(file) && !excludedFiles.includes(file))
    .sort()
    .map((file) => {
      const artifact = readFileSync(join(directory, file));
      let parsed: MigrationArtifact;
      try { parsed = JSON.parse(artifact.toString("utf8")); } catch { throw new Error(`invalid migration artifact ${file}`); }
      const idFromFile = Number(file.slice(0, 4));
      if (parsed.id !== undefined && (!Number.isSafeInteger(parsed.id) || parsed.id !== idFromFile)) {
        throw new Error(`migration artifact ${file} has invalid identity`);
      }
      const id = parsed.id ?? idFromFile;
      const name = parsed.name ?? legacyNames[id];
      if (typeof name !== "string" || !name || (parsed.sql !== undefined && typeof parsed.sql !== "string")) throw new Error(`migration artifact ${file} has invalid identity`);
      return { ...parsed, id, name, artifact, checksum: createHash("sha256").update(artifact).digest("hex") };
    });
  if (migrations.some((migration, index) => migration.id !== index + 1)) throw new Error("migration artifacts must form a contiguous sequence from 1");
  return Object.freeze(migrations);
}
