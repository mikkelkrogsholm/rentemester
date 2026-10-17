/** Compiled DDL-only recognized constraint variants and real TS normalization overlays. */
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { Database } from "bun:sqlite";
import { normalizeCompanyBaseline, migrate } from "../../src/core/db";
const root = new URL("../../", import.meta.url);
type ObjectRow = { kind: string; name: string; table: string; sql: string };
const imageRaw = readFileSync(
	new URL("crates/rentemester-storage/src/legacy-company-schemas.json", root),
	"utf8",
);
const images = JSON.parse(imageRaw) as {
	baseline: ObjectRow[];
	current: ObjectRow[];
	sources: unknown[];
	baseline_artifact_sha256: string;
};
const definitions = [
	{
		table: "accounting_periods",
		before: "('vat_period','vat_quarter','fiscal_year','custom')",
		after: "('vat_quarter','fiscal_year','custom')",
	},
	{ table: "gdpr_erasures", before: ",'audit_log'", after: "" },
	{
		table: "customers",
		before:
			"payment_terms_days INTEGER CHECK(payment_terms_days IS NULL OR payment_terms_days > 0)",
		after:
			"payment_terms_days INTEGER NOT NULL DEFAULT 30 CHECK(payment_terms_days > 0)",
	},
	{
		table: "companies",
		before: "vat_period_type TEXT DEFAULT 'quarter'",
		after: "vat_period_type TEXT NOT NULL DEFAULT 'quarter'",
	},
];
const objects = (db: Database) =>
	db
		.query(
			"SELECT type AS kind,name,tbl_name AS 'table',sql FROM sqlite_schema WHERE substr(name,1,7)<>'sqlite_' ORDER BY name",
		)
		.all() as ObjectRow[];
function delta(base: ObjectRow[], actual: ObjectRow[]) {
	const prior = new Map(base.map((o) => [o.name, o]));
	if (base.length !== actual.length)
		throw new Error("Unexpected schema cardinality change");
	return actual.filter(
		(o) => JSON.stringify(prior.get(o.name)) !== JSON.stringify(o),
	);
}
const rules = definitions.map((def) => {
	const original = images.baseline.find((o) => o.name === def.table)!.sql;
	let source_sql = original.replace(def.before, def.after);
	if (source_sql === original)
		throw new Error("Historical constraint fragment missing");
	if (def.table === "gdpr_erasures") {
		const replaced = source_sql.replace(
			"UNIQUE(subject_key, source, source_row_id)",
			"UNIQUE(source, source_row_id)",
		);
		if (replaced === source_sql)
			throw new Error("Historical GDPR uniqueness fragment missing");
		source_sql = replaced;
	}
	if (def.table === "companies") {
		const replaced = source_sql.replace("vat_period_type IS NULL OR ", "");
		if (replaced === source_sql)
			throw new Error("Historical company cadence CHECK fragment missing");
		source_sql = replaced;
	}
	const db = new Database(":memory:");
	db.exec("PRAGMA foreign_keys=ON");
	for (const o of images.baseline)
		db.exec(o.name === def.table ? source_sql : o.sql);
	normalizeCompanyBaseline(db);
	const baseline_delta = delta(images.baseline, objects(db));
	migrate(db);
	const current_delta = delta(images.current, objects(db));
	db.close();
	if (!baseline_delta.some((o) => o.name === def.table))
		throw new Error("Rebuild did not execute");
	return { table: def.table, source_sql, baseline_delta, current_delta };
});
const artifact = {
	format: "rentemester-company-physical-contract/1",
	baseline_artifact_sha256: images.baseline_artifact_sha256,
	schema_images_sha256: createHash("sha256").update(imageRaw).digest("hex"),
	sources: images.sources,
	rules,
};
const raw = JSON.stringify(artifact, null, 2) + "\n";
const destination = new URL(
		"crates/rentemester-storage/src/legacy-company-physical.json",
		root,
	),
	checksum = new URL(
		"crates/rentemester-storage/src/legacy-company-physical.sha256",
		root,
	);
const hash = createHash("sha256").update(raw).digest("hex") + "\n";
if (process.argv.includes("--write")) {
	await Bun.write(destination, raw);
	await Bun.write(checksum, hash);
} else if (
	(await Bun.file(destination).text()) !== raw ||
	(await Bun.file(checksum).text()) !== hash
)
	throw new Error(
		"Physical company contract drift; review before regenerating",
	);
