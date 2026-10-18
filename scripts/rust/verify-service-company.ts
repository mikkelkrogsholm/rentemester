/** Actual source-issued service credentials and real native company effects. */
import { Database } from "bun:sqlite";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { authorizeWorkspaceRoute } from "../../src/core/workspace-access";
import {
	copyFileSync,
	writeFileSync,
	existsSync,
	mkdtempSync,
	readFileSync,
	rmSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
import {
	createWorkspaceServicePrincipal,
	rotateWorkspaceServiceCredential,
} from "../../src/core/workspace-service-principals";
import { asJournalEntryId } from "../../src/core/ids";
import { reverseJournalEntryInCurrentTransaction } from "../../src/core/ledger";
import { migrate } from "../../src/core/db";
import {
	postJournalEntryInCurrentTransaction,
	seedAccounts,
	verifyAuditChain,
} from "../../src/core/ledger";
import { currentRuleBundleVersion } from "../../src/core/rules-metadata";
import {
	openWorkspaceControlDb,
	workspaceControlPaths,
} from "../../src/core/workspace-control";

const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo"),
	temp = mkdtempSync(join(tmpdir(), "rm-service-company-"));
const version = currentRuleBundleVersion();
const oldToday = process.env.RENTEMESTER_TODAY;
process.env.RENTEMESTER_TODAY = "2026-01-01";
const oldFuture = process.env.RENTEMESTER_MAX_FUTURE_DAYS;
process.env.RENTEMESTER_MAX_FUTURE_DAYS = "45";
let serial = 0;
const observations: { id: string }[] = [];
type Journal = {
	transactionDate: string;
	text: string;
	currency: string;
	documentId?: string;
	sourceBankTransactionId?: string;
	amountForeign?: string;
	amountDkk?: string;
	fxRateToDkk?: string;
	createdByProgram?: string;
	lines: {
		accountNo: string;
		debitAmount?: string;
		creditAmount?: string;
		vatCode?: string;
		text?: string | null;
	}[];
};
const base: Journal = {
	transactionDate: "2026-01-01",
	text: "Synthetic capital",
	currency: "DKK",
	lines: [
		{ accountNo: "2000", debitAmount: "10.00" },
		{ accountNo: "5000", creditAmount: "10.00" },
	],
};
function build(packageName: string) {
	const p = Bun.spawnSync(
		[
			cargo,
			"test",
			"--locked",
			"-p",
			packageName,
			"--lib",
			"--no-run",
			"--message-format=json",
		],
		{ cwd: root, stdout: "pipe", stderr: "pipe" },
	);
	assert.equal(p.exitCode, 0, p.stderr.toString());
	const helper = p.stdout
		.toString()
		.split("\n")
		.filter(Boolean)
		.map((s) => JSON.parse(s))
		.find(
			(v) => v.reason === "compiler-artifact" && v.profile.test && v.executable,
		)?.executable;
	assert.ok(helper);
	return helper as string;
}
const application = build("rentemester-application");
const controlSeed = join(temp, "control-seed"),
	control = openWorkspaceControlDb(controlSeed);
control.exec(
	"INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1); INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic'); INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','agent:synthetic')",
);
control
	.query(
		"INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES('s','u','synthetic-token',?,?,?)",
	)
	.run(
		"2026-01-01T00:00:00.000Z",
		"2026-01-01T00:00:00.000Z",
		"2026-01-01T12:00:00.000Z",
	);
control.query("PRAGMA wal_checkpoint(TRUNCATE)").all();
control.close(true);
const opts = {
	secret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
	trustedOrigins: ["https://example.invalid"],
	baseURL: "https://example.invalid",
	useSecureCookies: true,
};
const issuer = new Database(workspaceControlPaths(controlSeed).db),
	provider = createBetterAuthRuntime(issuer, opts);
const issued = await createWorkspaceServicePrincipal(issuer, provider, {
	displayName: "Synthetic company automation",
	actor: "agent:synthetic",
});
const { serviceAccountId: subject, credentialId, secret } = issued;
issuer
	.query(
		"INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?,'activate','member','agent:synthetic')",
	)
	.run(subject);
issuer
	.query(
		"INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?,'a','grant','bookkeeper','agent:synthetic')",
	)
	.run(subject);
issuer.query("PRAGMA wal_checkpoint(TRUNCATE)").all();
issuer.close(true);
const now = Date.now() + 2000;

function open(path: string) {
	const db = new Database(path, { safeIntegers: true });
	db.exec("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=10000");
	return db;
}
function fixture(mode = "wal", sql = "") {
	const path = join(temp, `company-${serial++}.sqlite`),
		db = new Database(path);
	db.exec("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=10000");
	migrate(db);
	seedAccounts(db);
	if (sql) db.exec(sql);
	db.exec(`PRAGMA journal_mode=${mode}`);
	db.close(true);
	const checkpoint = open(path);
	assert.equal(
		Number(
			(
				checkpoint.query("PRAGMA wal_checkpoint(TRUNCATE)").get() as {
					busy: bigint;
				}
			).busy,
		),
		0,
	);
	checkpoint.close(true);
	const c = join(temp, `control-${serial++}.sqlite`);
	copyFileSync(workspaceControlPaths(controlSeed).db, c);
	const cd = open(c);
	cd.exec(`PRAGMA journal_mode=${mode}`);
	cd.close(true);
	return { path, c };
}

function input(request: unknown = base, key?: string, operation = "post") {
	return {
		format:
			operation === "reverse"
				? "rentemester-manual-journal-reversal/1"
				: "rentemester-manual-journal-post/1",
		expectedRuleVersion: version,
		...(key ? { idempotencyKey: key } : {}),
		request,
	};
}
function config(
	request: unknown = base,
	key?: string,
	extra: Record<string, unknown> = {},
) {
	return {
		secret,
		now,
		input: input(request, key, extra.operation as string),
		...extra,
	};
}
const args = () => [
	application,
	"--ignored",
	"--exact",
	"service_company::tests::child",
	"--nocapture",
];
function processEnv(
	f: { path: string; c: string },
	v: unknown,
	out: string,
	extra: Record<string, string> = {},
) {
	return {
		...process.env,
		RM_SERVICE_COMPANY: f.path,
		RM_SERVICE_CONTROL: f.c,
		RM_SERVICE_REQUEST: JSON.stringify(v),
		RM_SERVICE_OUTPUT: out,
		...extra,
	};
}
function native(
	f: { path: string; c: string },
	request: unknown = base,
	key?: string,
	extra: Record<string, unknown> = {},
	extraEnv: Record<string, string> = {},
) {
	const out = join(temp, `report-${serial++}.json`),
		p = Bun.spawnSync(args(), {
			cwd: root,
			env: processEnv(f, config(request, key, extra), out, extraEnv),
			stdout: "pipe",
			stderr: "pipe",
		});
	assert.equal(p.exitCode, 0, p.stderr.toString() + p.stdout.toString());
	return JSON.parse(readFileSync(out, "utf8"));
}
function source(path: string, request: Journal = base) {
	const db = new Database(path);
	db.exec("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL");
	const wire = {
		...request,
		createdBy: `user:${subject}`,
		createdByProgram: "service-principal",
		documentId:
			request.documentId === undefined ? undefined : Number(request.documentId),
		sourceBankTransactionId:
			request.sourceBankTransactionId === undefined
				? undefined
				: Number(request.sourceBankTransactionId),
		amountForeign:
			request.amountForeign === undefined
				? undefined
				: Number(request.amountForeign),
		amountDkk:
			request.amountDkk === undefined ? undefined : Number(request.amountDkk),
		fxRateToDkk:
			request.fxRateToDkk === undefined
				? undefined
				: Number(request.fxRateToDkk),
		lines: request.lines.map((line) => ({
			...line,
			text: line.text ?? undefined,
			debitAmount: Number(line.debitAmount ?? 0),
			creditAmount: Number(line.creditAmount ?? 0),
		})),
	};
	try {
		return db
			.transaction(() => postJournalEntryInCurrentTransaction(db, wire))
			.immediate();
	} finally {
		db.close(true);
	}
}
function encoded(v: unknown) {
	return JSON.stringify(v, (_, v) =>
		typeof v === "bigint" ? { $integer: v.toString() } : v,
	);
}
function tables(path: string, mask = false) {
	const db = open(path);
	try {
		return (
			db
				.query(
					"SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
				)
				.all() as { name: string }[]
		).map(({ name }) => [
			name,
			db
				.query(`SELECT * FROM "${name.replaceAll('"', '""')}"`)
				.all()
				.map((r) => {
					const row = { ...(r as Record<string, unknown>) };
					if (mask) {
						if (
							["journal_entries", "journal_lines", "audit_log"].includes(name)
						)
							delete row.created_at;
						if (name === "journal_entries") {
							assert.match(
								row.registration_datetime as string,
								/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/,
							);
							delete row.registration_datetime;
						}
						if (name === "exceptions") delete row.resolved_at;
					}
					return row;
				}),
		]);
	} finally {
		db.close(true);
	}
}
function snapshot(path: string) {
	return encoded(tables(path));
}
function checked(id: string, f: () => void) {
	if (process.env.RM_SERVICE_PROGRESS === "1") process.stderr.write(id + "\n");
	f();
	observations.push({ id });
}
function count(path: string, table: string) {
	const db = open(path);
	try {
		return Number(
			(db.query(`SELECT count(*) n FROM ${table}`).get() as { n: bigint }).n,
		);
	} finally {
		db.close(true);
	}
}
function accepted(v: any) {
	assert.equal(v.ok, true, encoded(v));
	assert.equal(v.usage, "committed");
	assert.equal(v.business, "committed");
	assert.equal(v.result.ok, true, encoded(v));
	return v.result;
}
async function waitFor(path: string) {
	const until = Date.now() + 10000;
	while (!existsSync(path)) {
		assert.ok(Date.now() < until, "native checkpoint missing");
		await Bun.sleep(10);
	}
}
const documentSql =
	"INSERT INTO documents(id,source,sha256_hash,document_type) VALUES(1,'synthetic','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','purchase_sale')";
const bankSql =
	"INSERT INTO bank_transactions(id,transaction_date,text,amount) VALUES(1,'2026-01-01','Synthetic bank',10); INSERT INTO exceptions(type,message,related_bank_transaction_id) VALUES('UNMATCHED_BANK_TRANSACTION','Synthetic unmatched',1),('OTHER','Synthetic untouched',1)";
writeFileSync(
	join(temp, "workspace.json"),
	JSON.stringify({
		version: 1,
		companies: [
			{
				slug: "a",
				name: "Synthetic",
				archived: false,
				createdAt: new Date(now).toISOString(),
			},
		],
	}),
);
const reverse = {
	entryId: "1",
	transactionDate: "2026-01-02",
	reason: "Synthetic correction",
};
function controlDb<T>(f: { c: string }, fn: (db: Database) => T) {
	const db = new Database(f.c);
	try {
		return fn(db);
	} finally {
		db.close(true);
	}
}
function controlImage(f: { c: string }) {
	const data = tables(f.c);
	for (const [name, rows] of data)
		if (name === "apikey")
			for (const row of rows as Record<string, unknown>[])
				if (row.id === credentialId)
					for (const k of [
						"remaining",
						"requestCount",
						"lastRefillAt",
						"lastRequest",
						"updatedAt",
					])
						delete row[k];
	return createHash("sha256").update(encoded(data)).digest("hex");
}
function sourceAllowed(f: { c: string }) {
	return controlDb(
		f,
		(db) =>
			authorizeWorkspaceRoute(db, temp, {
				userId: subject,
				permission: "company.ledger.post",
				companySlug: "a",
			}).allowed,
	);
}
function usage(f: { c: string }, id = credentialId) {
	return controlDb(f, (db) =>
		db.query("SELECT * FROM apikey WHERE id=?").get(id),
	) as any;
}
function update(f: { c: string }, fields: Record<string, unknown>) {
	if (!Object.keys(fields).length) return;
	controlDb(f, (db) =>
		db
			.query(
				`UPDATE apikey SET ${Object.keys(fields)
					.map((k) => `"${k}"=?`)
					.join(",")} WHERE id=?`,
			)
			.run(...(Object.values(fields) as any[]), credentialId),
	);
}
async function atSourceClock<T>(f: () => Promise<T>) {
	const RealDate = globalThis.Date;
	class Fixed extends RealDate {
		constructor(v?: any) {
			super(v === undefined ? now : v);
		}
		static now() {
			return now;
		}
	}
	globalThis.Date = Fixed as typeof Date;
	try {
		return await f();
	} finally {
		globalThis.Date = RealDate;
	}
}
async function sourceVerify(f: { c: string }) {
	const db = new Database(f.c),
		auth = createBetterAuthRuntime(db, opts);
	try {
		return await atSourceClock<{ valid: boolean }>(() =>
			(auth.api as any).verifyApiKey({
				body: { key: secret, configId: "workspace-service-principal" },
			}),
		);
	} finally {
		db.close(true);
	}
}
function originalOracle(f: { path: string; c: string }) {
	const db = open(f.path);
	db.query("PRAGMA wal_checkpoint(TRUNCATE)").all();
	db.close(true);
	const path = join(temp, `oracle-${serial++}`);
	copyFileSync(f.path, path);
	return path;
}
function sourceReverse(path: string) {
	const db = new Database(path);
	try {
		return db
			.transaction(() =>
				reverseJournalEntryInCurrentTransaction(db, {
					entryId: asJournalEntryId(1),
					transactionDate: reverse.transactionDate,
					reason: reverse.reason,
					createdBy: `user:${subject}`,
					createdByProgram: "service-principal",
				}),
			)
			.immediate();
	} finally {
		db.close(true);
	}
}
function usagePreserved(f: { c: string }, before: any) {
	const after = usage(f);
	for (const row of [before, after])
		for (const k of [
			"remaining",
			"requestCount",
			"lastRefillAt",
			"lastRequest",
			"updatedAt",
		])
			delete row[k];
	assert.deepEqual(after, before);
}
try {
	for (const mode of ["wal", "delete"]) {
		checked(`${mode}/real-post-source-company-and-source-issued-key`, () => {
			const f = fixture(mode),
				p = originalOracle(f),
				before = usage(f),
				beforeControl = controlImage(f),
				expected = source(p),
				actual = accepted(native(f));
			assert.deepEqual(actual, expected);
			assert.equal(encoded(tables(f.path, true)), encoded(tables(p, true)));
			assert.equal(usage(f).requestCount, before.requestCount + 1);
			usagePreserved(f, before);
			assert.equal(controlImage(f), beforeControl);
			assert.equal(sourceAllowed(f), true);
			const db = new Database(f.path);
			assert.equal(verifyAuditChain(db).ok, true);
			db.close(true);
		});
		checked(`${mode}/real-reversal-source-company`, () => {
			const f = fixture(mode);
			accepted(native(f));
			const p = originalOracle(f),
				expected = sourceReverse(p),
				actual = accepted(
					native(f, reverse, undefined, { operation: "reverse" }),
				);
			assert.deepEqual(actual, expected);
			assert.equal(encoded(tables(f.path, true)), encoded(tables(p, true)));
			assert.equal(usage(f).requestCount, 2);
			const db = new Database(f.path);
			assert.equal(verifyAuditChain(db).ok, true);
			db.close(true);
		});
		for (const [name, fields, expected] of [
			[
				"last-quota-and-rate-slot",
				{
					remaining: 1,
					requestCount: 9,
					rateLimitMax: 10,
					rateLimitTimeWindow: 10000,
					lastRequest: new Date(now - 1).toISOString(),
				},
				true,
			],
			["quota-empty", { remaining: 0 }, false],
			[
				"rate-full",
				{
					requestCount: 10,
					rateLimitMax: 10,
					rateLimitTimeWindow: 10000,
					lastRequest: new Date(now - 1).toISOString(),
				},
				false,
			],
			["disabled", { enabled: 0 }, false],
			["expired", { expiresAt: new Date(now).toISOString() }, false],
			["wrong-config", { configId: "other" }, false],
			["malformed-count", { requestCount: -1 }, false],
		] as [string, Record<string, unknown>, boolean][]) {
			checked(`${mode}/${name}`, () => {
				const f = fixture(mode);
				update(f, fields);
				const before = snapshot(f.c),
					company = snapshot(f.path),
					n = native(f);
				if (expected) {
					accepted(n);
					assert.equal(usage(f).remaining, 0);
					assert.equal(usage(f).requestCount, 10);
				} else {
					assert.equal(n.ok, false);
					assert.equal(n.usage, "not_committed");
					assert.equal(n.business, "not_attempted");
					assert.equal(snapshot(f.c), before);
					assert.equal(snapshot(f.path), company);
				}
			});
		}
		for (const [name, fields] of [
			["default", {}],
			["rate-disabled", { rateLimitEnabled: 0, remaining: 2, requestCount: 4 }],
			[
				"refill-exact",
				{
					remaining: 2,
					refillInterval: 1000,
					refillAmount: 5,
					lastRefillAt: new Date(now - 1000).toISOString(),
				},
			],
			[
				"refill-plus-one",
				{
					remaining: 0,
					refillInterval: 1000,
					refillAmount: 5,
					lastRefillAt: new Date(now - 1001).toISOString(),
				},
			],
			[
				"inactivity-exact",
				{
					requestCount: 2,
					rateLimitMax: 5,
					rateLimitTimeWindow: 1000,
					lastRequest: new Date(now - 1000).toISOString(),
				},
			],
			[
				"inactivity-plus-one",
				{
					requestCount: 2,
					rateLimitMax: 5,
					rateLimitTimeWindow: 1000,
					lastRequest: new Date(now - 1001).toISOString(),
				},
			],
		] as [string, Record<string, unknown>][]) {
			const a = fixture(mode),
				b = fixture(mode);
			update(a, fields);
			update(b, fields);
			if (name === "inactivity-exact") {
				const n = native(b);
				checked(
					`${mode}/native-inactivity-exact-known-source-CAS-correction`,
					() => {
						accepted(n);
						assert.equal(usage(b).requestCount, 3);
						assert.equal(usage(b).lastRequest, new Date(now).toISOString());
					},
				);
			} else {
				const oracle = await sourceVerify(a),
					n = native(b);
				checked(`${mode}/provider-usage-${name}`, () => {
					assert.equal(oracle.valid, true, encoded(oracle));
					accepted(n);
					for (const key of [
						"remaining",
						"requestCount",
						"lastRequest",
						"lastRefillAt",
						"updatedAt",
					])
						assert.deepEqual(usage(b)[key], usage(a)[key], key);
				});
			}
		}
		for (const [name, sql] of [
			[
				"reader",
				`INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('${subject}','a','grant','reader','agent:synthetic')`,
			],
			[
				"workspace-disabled",
				`INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('${subject}','disable',NULL,'agent:synthetic')`,
			],
			[
				"membership-revoked",
				`INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('${subject}','a','revoke',NULL,'agent:synthetic')`,
			],
			[
				"owner-no-company",
				`INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('${subject}','activate','workspace_owner','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('${subject}','a','revoke',NULL,'agent:synthetic')`,
			],
		])
			checked(`${mode}/authority-${name}`, () => {
				const f = fixture(mode);
				controlDb(f, (db) => db.exec(sql));
				assert.equal(sourceAllowed(f), false);
				const before = snapshot(f.c),
					company = snapshot(f.path),
					n = native(f);
				assert.equal(n.ok, false);
				assert.equal(n.usage, "not_committed");
				assert.equal(snapshot(f.c), before);
				assert.equal(snapshot(f.path), company);
			});
		for (const [name, extra] of Object.entries({
			archive: { archived: true },
			unregistered: { registered: false },
			workspace: { workspace: "other" },
			keyworkspace: { credentialWorkspace: "other" },
		}))
			checked(`${mode}/selection-${name}`, () => {
				const f = fixture(mode),
					before = snapshot(f.c),
					company = snapshot(f.path);
				assert.equal(native(f, base, undefined, extra).usage, "not_committed");
				assert.equal(snapshot(f.c), before);
				assert.equal(snapshot(f.path), company);
			});
		for (const field of [
			"controlQueryOnly",
			"companyQueryOnly",
			"controlTransaction",
			"companyTransaction",
		])
			checked(`${mode}/preflight-${field}`, () => {
				const f = fixture(mode),
					before = snapshot(f.c),
					company = snapshot(f.path),
					n = native(f, base, undefined, { [field]: true });
				assert.equal(n.usage, "not_committed");
				assert.equal(n.business, "not_attempted");
				assert.equal(snapshot(f.c), before);
				assert.equal(snapshot(f.path), company);
			});
		for (const [name, request] of [
			["wrong-format", { ...input(), format: "unsupported" }],
			[
				"reserved-program",
				input({ ...base, createdByProgram: "rentemester-import-postings" }),
			],
			["bad-key", { ...input(), idempotencyKey: " " }],
		] as [string, unknown][])
			checked(`${mode}/invalid-${name}`, () => {
				const f = fixture(mode),
					before = snapshot(f.c),
					company = snapshot(f.path);
				assert.equal(
					native(f, base, undefined, { input: request }).usage,
					"not_committed",
				);
				assert.equal(snapshot(f.c), before);
				assert.equal(snapshot(f.path), company);
			});
		checked(`${mode}/charged-business-refusal`, () => {
			const f = fixture(mode),
				before = snapshot(f.path);
			const n = native(
				f,
				{
					...base,
					lines: [
						{ accountNo: "2000", debitAmount: "10.00" },
						{ accountNo: "5000", creditAmount: "9.00" },
					],
				},
				"bad",
			);
			assert.equal(n.ok, false);
			assert.equal(n.usage, "committed");
			assert.equal(n.business, "not_committed");
			assert.equal(n.rejection.ok, false);
			assert.equal(usage(f).requestCount, 1);
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/charged-replay-and-conflict`, () => {
			const f = fixture(mode),
				first = accepted(native(f, base, "same")),
				replay = native(f, base, "same");
			assert.deepEqual(accepted(replay), first);
			assert.equal(replay.receipt.replayed, true);
			const conflict = native(f, { ...base, text: "changed" }, "same");
			assert.equal(conflict.ok, true);
			assert.equal(conflict.result.ok, false);
			assert.equal(usage(f).requestCount, 3);
			assert.equal(count(f.path, "journal_entries"), 1);
		});
		const rotated = fixture(mode);
		const first = accepted(native(rotated, base, "rotation"));
		const rd = new Database(rotated.c),
			replacement = await atSourceClock(() =>
				rotateWorkspaceServiceCredential(
					rd,
					createBetterAuthRuntime(rd, opts),
					{ serviceAccountId: subject, credentialId, actor: "agent:synthetic" },
				),
			);
		rd.close(true);
		checked(`${mode}/rotated-key-current-auth-and-receipt`, () => {
			const n = native(rotated, base, "rotation", {
				secret: replacement.secret,
			});
			assert.deepEqual(accepted(n), first);
			assert.equal(n.receipt.replayed, true);
			assert.equal(native(rotated, base, "rotation").usage, "not_committed");
			assert.equal(count(rotated.path, "journal_entries"), 1);
		});
		checked(`${mode}/usage-sql-abort-no-company-attempt`, () => {
			const f = fixture(mode),
				before = snapshot(f.c),
				company = snapshot(f.path),
				n = native(f, base, undefined, {}, { RM_SERVICE_FAIL: "usage" });
			assert.equal(n.usage, "not_committed");
			assert.equal(n.business, "not_attempted");
			assert.equal(snapshot(f.c), before);
			assert.equal(snapshot(f.path), company);
		});
		checked(`${mode}/company-sql-abort-retains-usage`, () => {
			const f = fixture(mode),
				before = snapshot(f.path),
				n = native(f, base, "late", {}, { RM_SERVICE_FAIL: "company" });
			assert.equal(n.usage, "committed");
			assert.equal(n.business, "not_committed");
			assert.equal(usage(f).requestCount, 1);
			assert.equal(snapshot(f.path), before);
			accepted(native(f, base, "late"));
			assert.equal(usage(f).requestCount, 2);
			assert.equal(count(f.path, "journal_entries"), 1);
		});
		checked(`${mode}/expiry-between-phases-retains-charge`, () => {
			const f = fixture(mode);
			update(f, { expiresAt: new Date(now + 1).toISOString() });
			const n = native(f, base, undefined, { clockStep: 2 });
			assert.equal(n.usage, "committed");
			assert.equal(n.business, "not_attempted");
			assert.equal(usage(f).requestCount, 1);
			assert.equal(count(f.path, "journal_entries"), 0);
		});
		for (const action of ["revoke", "disable-key"]) {
			const f = fixture(mode),
				marker = join(temp, `gap-${serial++}`),
				out = marker + ".json";
			const child = Bun.spawn(args(), {
				cwd: root,
				env: processEnv(f, config(base, "gap"), out, {
					RM_SERVICE_POINT: "usage-committed",
					RM_SERVICE_MARKER: marker,
				}),
				stdin: "pipe",
				stdout: "pipe",
				stderr: "pipe",
			});
			await waitFor(marker);
			if (action === "revoke")
				controlDb(f, (db) =>
					db
						.query(
							"INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?,'a','revoke',NULL,'agent:synthetic')",
						)
						.run(subject),
				);
			else update(f, { enabled: 0 });
			child.stdin.write("continue\n");
			child.stdin.end();
			assert.equal(await child.exited, 0);
			checked(`${mode}/gap-${action}`, () => {
				const n = JSON.parse(readFileSync(out, "utf8"));
				assert.equal(n.usage, "committed");
				assert.equal(n.business, "not_attempted");
				assert.equal(n.result, undefined);
				assert.equal(usage(f).requestCount, 1);
				assert.equal(count(f.path, "journal_entries"), 0);
			});
		}
		{
			const f = fixture(mode),
				marker = join(temp, `held-revoke-${serial++}`),
				out = marker + ".json";
			const a = Bun.spawn(args(), {
				cwd: root,
				env: processEnv(f, config(base, "held", { busyMs: 10000 }), out, {
					RM_FENCE_POINT: "precommit",
					RM_FENCE_MARKER: marker,
				}),
				stdin: "pipe",
				stdout: "pipe",
				stderr: "pipe",
			});
			await waitFor(marker);
			const code = `const {Database}=require('bun:sqlite');const db=new Database(process.argv[1]);db.exec('PRAGMA foreign_keys=ON;PRAGMA busy_timeout=10000');db.query("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?,'a','revoke',NULL,'agent:synthetic')").run(process.argv[2]);db.close(true);`;
			const revoker = Bun.spawn([process.execPath, "-e", code, f.c, subject], {
				stdout: "pipe",
				stderr: "pipe",
			});
			await Bun.sleep(50);
			assert.equal(revoker.exitCode, null);
			a.stdin.write("continue\n");
			a.stdin.end();
			assert.equal(await a.exited, 0);
			assert.equal(
				await revoker.exited,
				0,
				await new Response(revoker.stderr).text(),
			);
			checked(`${mode}/revoker-waits-for-protected-company-commit`, () => {
				accepted(JSON.parse(readFileSync(out, "utf8")));
				const n = native(f, base, "held");
				assert.equal(n.usage, "not_committed");
				assert.equal(n.result, undefined);
				assert.equal(count(f.path, "journal_entries"), 1);
				assert.equal(usage(f).requestCount, 1);
			});
		}
		for (const point of [
			"usage-precommit",
			"usage-committed",
			"precommit",
			"postcommit",
		]) {
			const f = fixture(mode),
				marker = join(temp, `kill-${serial++}`),
				out = marker + ".json",
				service = point.startsWith("usage");
			const child = Bun.spawn(args(), {
				cwd: root,
				env: processEnv(
					f,
					config(base, "lost"),
					out,
					service
						? { RM_SERVICE_POINT: point, RM_SERVICE_MARKER: marker }
						: { RM_FENCE_POINT: point, RM_FENCE_MARKER: marker },
				),
				stdin: "pipe",
				stdout: "pipe",
				stderr: "pipe",
			});
			await waitFor(marker);
			child.kill("SIGKILL");
			await child.exited;
			checked(`${mode}/death-${point}`, () => {
				assert.equal(existsSync(out), false);
				assert.equal(
					usage(f).requestCount,
					point === "usage-precommit" ? 0 : 1,
				);
				assert.equal(
					count(f.path, "journal_entries"),
					point === "postcommit" ? 1 : 0,
				);
				const n = native(f, base, "lost");
				accepted(n);
				assert.equal(n.receipt.replayed, point === "postcommit");
				assert.equal(count(f.path, "journal_entries"), 1);
			});
		}
		for (const limited of [true, false]) {
			const f = fixture(mode);
			if (limited) update(f, { remaining: 1 });
			const marker = join(temp, `concurrent-${serial++}`),
				outA = marker + "a",
				outB = marker + "b";
			const a = Bun.spawn(args(), {
				cwd: root,
				env: processEnv(
					f,
					config(base, "concurrent", { busyMs: 10000 }),
					outA,
					{ RM_FENCE_POINT: "precommit", RM_FENCE_MARKER: marker },
				),
				stdin: "pipe",
				stdout: "pipe",
				stderr: "pipe",
			});
			await waitFor(marker);
			const b = Bun.spawn(args(), {
				cwd: root,
				env: processEnv(f, config(base, "concurrent", { busyMs: 10000 }), outB),
				stdout: "pipe",
				stderr: "pipe",
			});
			await Bun.sleep(50);
			assert.equal(b.exitCode, null);
			a.stdin.write("continue\n");
			a.stdin.end();
			assert.equal(await a.exited, 0);
			assert.equal(await b.exited, 0);
			checked(
				`${mode}/concurrent-${limited ? "last-quota" : "same-key"}`,
				() => {
					accepted(JSON.parse(readFileSync(outA, "utf8")));
					const n = JSON.parse(readFileSync(outB, "utf8"));
					if (limited) {
						assert.equal(n.usage, "not_committed");
						assert.equal(usage(f).remaining, 0);
					} else {
						accepted(n);
						assert.equal(n.receipt.replayed, true);
					}
					assert.equal(count(f.path, "journal_entries"), 1);
					assert.equal(usage(f).requestCount, limited ? 1 : 2);
				},
			);
		}
	}
	for (const target of ["usage", "company"])
		checked(`actual-delete-${target}-commit-unknown`, () => {
			const f = fixture("delete"),
				reader = open(target === "usage" ? f.c : f.path);
			reader.exec("BEGIN");
			reader.query("SELECT * FROM sqlite_schema").all();
			const n = native(f, base, "unknown");
			assert.equal(n.ok, false);
			assert.equal(
				n.usage,
				target === "usage" ? "outcome_unknown" : "committed",
			);
			assert.equal(
				n.business,
				target === "usage" ? "not_attempted" : "outcome_unknown",
			);
			assert.equal(n.result, undefined);
			reader.exec("ROLLBACK");
			reader.close(true);
		});
	console.log(
		JSON.stringify(
			{
				ok: true,
				scope:
					"source-issued service keys, source quota observations and actual native post/reversal under two-stage charged-attempt company authorization; no deployed writer authority",
				checks: observations.length,
				observations,
			},
			null,
			2,
		),
	);
} finally {
	if (oldToday === undefined) delete process.env.RENTEMESTER_TODAY;
	else process.env.RENTEMESTER_TODAY = oldToday;
	if (oldFuture === undefined) delete process.env.RENTEMESTER_MAX_FUTURE_DAYS;
	else process.env.RENTEMESTER_MAX_FUTURE_DAYS = oldFuture;
	rmSync(temp, { recursive: true, force: true });
}
