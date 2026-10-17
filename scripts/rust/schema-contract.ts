import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { validateWorkspaceControlMigrationHistory } from "../../src/core/workspace-control";
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

// Bind all control identities to the real TypeScript catalogue before generation.
const directory = new URL(
	"../../src/core/workspace-migrations/",
	import.meta.url,
);
const control = readdirSync(directory)
	.filter((f) => f.endsWith(".json"))
	.sort()
	.map((file) => {
		const bytes = readFileSync(new URL(file, directory));
		const artifact = JSON.parse(bytes.toString("utf8"));
		return {
			id: artifact.id,
			name: artifact.name,
			checksum: createHash("sha256").update(bytes).digest("hex"),
		};
	});
validateWorkspaceControlMigrationHistory(control);
const controlDestination = new URL(
	"../../crates/rentemester-storage/src/legacy-workspace-schema.json",
	import.meta.url,
);
const controlExpected = `${JSON.stringify(control, null, 2)}\n`;
if (process.argv.includes("--write"))
	await Bun.write(controlDestination, controlExpected);
else if ((await Bun.file(controlDestination).text()) !== controlExpected)
	throw new Error(
		"Rust workspace-control catalogue drifted from exact TypeScript artifacts",
	);
