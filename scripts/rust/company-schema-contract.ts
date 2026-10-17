/** Reproducible empty-schema images; never contains company rows or identities. */
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { Database } from "bun:sqlite";
import { normalizeCompanyBaseline, migrate } from "../../src/core/db";
import { supportedSchemaMigrations } from "../../src/core/schema-version";
const root = new URL("../../", import.meta.url);
const sourcePaths = [
	"src/core/schema.sql",
	"src/core/db.ts",
	"src/core/companies-schema.ts",
	"src/core/schema-version.ts",
	"src/core/retention.ts",
	"src/core/account-roles.ts",
];
const sources = sourcePaths.map((path) => ({
	path,
	sha256: createHash("sha256")
		.update(readFileSync(new URL(path, root)))
		.digest("hex"),
}));
const db = new Database(":memory:");
db.exec("PRAGMA foreign_keys=ON");
const objects = (order: string) =>
	db
		.query(
			`SELECT type AS kind,name,tbl_name AS 'table',sql FROM sqlite_schema WHERE substr(name,1,7)<>'sqlite_' ORDER BY ${order}`,
		)
		.all();
normalizeCompanyBaseline(db);
const baseline = objects("rowid");
migrate(db);
const current = objects("name");
db.close();
const artifact = {
	format: "rentemester-company-schema-images/1",
	baseline_artifact_sha256: supportedSchemaMigrations()[0]!.checksum,
	sources,
	baseline,
	current,
};
const expected = JSON.stringify(artifact, null, 2) + "\n",
	destination = new URL(
		"../../crates/rentemester-storage/src/legacy-company-schemas.json",
		import.meta.url,
	);
if (process.argv.includes("--write")) await Bun.write(destination, expected);
else if ((await Bun.file(destination).text()) !== expected)
	throw new Error(
		"Company schema image drift; review exact baseline/final guards before regenerating",
	);

const checksumDestination = new URL(
	"../../crates/rentemester-storage/src/legacy-company-schemas.sha256",
	import.meta.url,
);
const checksum = createHash("sha256").update(expected).digest("hex") + "\n";
if (process.argv.includes("--write"))
	await Bun.write(checksumDestination, checksum);
else if ((await Bun.file(checksumDestination).text()) !== checksum)
	throw new Error("Company schema image checksum drift");
