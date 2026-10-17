/** Independent expectations and actual legacy control-store policy; no product endpoint. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	mkdtempSync,
	readFileSync,
	writeFileSync,
	mkdirSync,
	rmSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { authorizeWorkspaceRoute } from "../../src/core/workspace-access";
import {
	ALL_ROUTE_PERMISSIONS,
	roleAllowsPermission,
} from "../../src/core/access-permissions";
const root = resolve(import.meta.dir, "../.."),
	dir = join(
		root,
		"docs/architecture/rust-target/implementation/access-policy",
	);
const hash = (s: string | Buffer) =>
	createHash("sha256").update(s).digest("hex");
const manifest = JSON.parse(
	readFileSync(join(dir, "vectors-manifest.json"), "utf8"),
);
const input = readFileSync(join(dir, "vectors.jsonl"), "utf8");
assert.equal(hash(input), manifest.vectors_sha256);
assert.equal(
	hash(readFileSync(join(root, "scripts/rust/access-policy-vectors.py"))),
	manifest.generator_sha256,
);
for (const [file, sha] of Object.entries(manifest.source_sha256))
	assert.equal(hash(readFileSync(join(root, file))), sha);
const cases = input
	.trimEnd()
	.split("\n")
	.map((line) => JSON.parse(line));
assert.equal(cases.length, manifest.checks);
assert.deepEqual(
	[...new Set(cases.map((v) => v.permission))],
	ALL_ROUTE_PERMISSIONS,
);
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
		"-p",
		"rentemester-application",
		"--test",
		"access_vectors",
		"--no-run",
		"--message-format=json",
	],
	{ cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const helper = build.stdout
	.toString()
	.trim()
	.split("\n")
	.map((s) => JSON.parse(s))
	.find(
		(v) =>
			v.reason === "compiler-artifact" &&
			v.target.name === "access_vectors" &&
			v.executable,
	)?.executable;
assert(helper);
const work = mkdtempSync(join(tmpdir(), "rm-access-")),
	native = join(work, "native.jsonl"),
	db = openWorkspaceControlDb(work);
try {
	const file = join(work, "workspace.json"),
		sentinel = join(work, "company-a/data/ledger.sqlite");
	mkdirSync(join(work, "company-a/data"), { recursive: true });
	writeFileSync(sentinel, "synthetic unopened ledger sentinel\n");
	const sentinelHash = hash(readFileSync(sentinel));
	let legacyDifferences = 0;
	const observations = [];
	for (const v of cases) {
		const id = `synthetic-${v.id}`,
			f = v.fault;
		db.transaction(() => {
			if (v.kind !== "missing") {
				db.query(
					'INSERT INTO "user"(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES(?,?,?,?,?,?,?)',
				).run(
					id,
					id,
					`${id}@example.test`,
					v.verified ? 1 : 0,
					v.mfa ? 1 : 0,
					"2026-01-01",
					"2026-01-01",
				);
				if (v.service)
					db.query(
						"INSERT INTO rm_workspace_service_principals(user_id,display_name) VALUES(?,?)",
					).run(id, id);
				if (f !== "no-workspace") {
					db.query(
						"INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor,created_at) VALUES(?,'activate',?,'agent:synthetic','2099-01-01')",
					).run(id, v.workspaceRole);
					if (f === "disabled")
						db.query(
							"INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor,created_at) VALUES(?,'disable',NULL,'agent:synthetic','2000-01-01')",
						).run(id);
				}
				if (v.companyRole) {
					db.query(
						"INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor,created_at) VALUES(?,'company-a','grant',?,'agent:synthetic','2099-01-01')",
					).run(id, v.companyRole);
					if (f === "revoked")
						db.query(
							"INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor,created_at) VALUES(?,'company-a','revoke',NULL,'agent:synthetic','2000-01-01')",
						).run(id);
				}
			}
		}).immediate();
		writeFileSync(
			file,
			JSON.stringify({
				version: 1,
				companies: ["no-company", "unregistered"].includes(f)
					? []
					: [
							{
								slug: "company-a",
								name: "Synthetic A",
								createdAt: "2026-01-01",
								archived: f === "archived",
							},
						],
			}),
		);
		const before = hash(db.serialize()),
			changes = JSON.stringify(db.query("SELECT total_changes() AS n").get()),
			manifestHash = hash(readFileSync(file));
		const result = authorizeWorkspaceRoute(db, work, {
			userId: v.kind === "missing" ? null : id,
			permission: v.permission,
			companySlug:
				f === "request-company"
					? "company-b"
					: f === "bad-slug"
						? "../a"
						: f === "no-request-company"
							? null
							: "company-a",
		});
		assert.equal(
			result.allowed,
			v.legacyExpected,
			`${v.id} actual legacy core`,
		);
		assert.equal(hash(db.serialize()), before);
		assert.equal(
			JSON.stringify(db.query("SELECT total_changes() AS n").get()),
			changes,
		);
		assert.equal(hash(readFileSync(file)), manifestHash);
		assert.equal(hash(readFileSync(sentinel)), sentinelHash);
		if (v.expected !== result.allowed) legacyDifferences++;
		observations.push({
			id: v.id,
			legacy: result.allowed,
			nativeExpected: v.expected,
		});
		assert.equal(
			roleAllowsPermission(v.workspaceRole, v.permission),
			v.workspaceRoleExpected,
		);
		assert.equal(
			roleAllowsPermission(v.companyRole, v.permission),
			v.companyRoleExpected,
		);
	}
	assert.equal(legacyDifferences, manifest.legacy_core_differences);
	for (const env of [
		{ TZ: "UTC" },
		{
			TZ: "Pacific/Honolulu",
			RENTEMESTER_ACTOR: "system:untrusted",
			USER: "untrusted",
		},
	]) {
		const run = Bun.spawnSync(
			[helper, "--ignored", "--exact", "vectors", "--nocapture"],
			{
				cwd: work,
				env: {
					...process.env,
					...env,
					RM_ACCESS_INPUT: join(dir, "vectors.jsonl"),
					RM_ACCESS_OUTPUT: native,
				},
				stdout: "pipe",
				stderr: "pipe",
			},
		);
		assert.equal(
			run.exitCode,
			0,
			run.stdout.toString() + run.stderr.toString(),
		);
		const values = readFileSync(native, "utf8")
			.trimEnd()
			.split("\n")
			.map((s) => JSON.parse(s));
		assert.equal(values.length, cases.length);
		for (let i = 0; i < values.length; i++)
			assert.deepEqual(values[i], {
				id: cases[i].id,
				allowed: cases[i].expected,
			});
	}
	const frozen = observations.map((v) => JSON.stringify(v)).join("\n") + "\n",
		path = join(dir, "legacy-observations.jsonl");
	if (process.argv.includes("--capture"))
		writeFileSync(path, frozen, { flag: "wx" });
	else assert.equal(readFileSync(path, "utf8"), frozen);
	console.log(
		JSON.stringify(
			{
				ok: true,
				checks: cases.length + 1,
				sourceCases: cases.length,
				legacyCoreDifferences: legacyDifferences,
				nativeEnvironments: 2,
				legacyObservationsSha256: hash(frozen),
				limits: [
					"Pure trusted-observation policy, not identity verification or revocation fencing",
					"Legacy core assumes upstream authentication; explicit binding/live-proof differences retained",
					"Synthetic current control data only; no company ledger opened",
				],
			},
			null,
			2,
		),
	);
} finally {
	db.close();
	rmSync(work, { recursive: true, force: true });
}
