/** Actual source/native manual posts on canonical synthetic company/control databases. */
import { Database } from "bun:sqlite";
import assert from "node:assert/strict";
import {
	copyFileSync,
	existsSync,
	mkdtempSync,
	readFileSync,
	rmSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
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
	temp = mkdtempSync(join(tmpdir(), "rm-real-post-"));
const corpus = JSON.parse(
	readFileSync(
		join(root, "crates/rentemester-runtime/fixtures/session-cookie.json"),
		"utf8",
	),
);
const now = Date.parse("2026-01-01T01:00:00.000Z"),
	version = currentRuleBundleVersion();
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
const application = build("rentemester-application"),
	storage = build("rentemester-storage");
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
function input(request: Journal = base, key?: string) {
	return {
		format: "rentemester-manual-journal-post/1",
		expectedRuleVersion: version,
		...(key ? { idempotencyKey: key } : {}),
		request,
	};
}
function config(
	request: Journal,
	key?: string,
	extra: Record<string, unknown> = {},
) {
	return {
		signer: corpus.key,
		cookie: corpus.vectors[0].header,
		now,
		input: input(request, key),
		...extra,
	};
}
const args = (helper: string) => [
	helper,
	"--ignored",
	"--exact",
	helper === application
		? "journal_post::tests::child"
		: "journal_post::tests::child",
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
		RM_POST_COMPANY: f.path,
		RM_POST_CONTROL: f.c,
		RM_POST_REQUEST: JSON.stringify(v),
		RM_POST_OUTPUT: out,
		...extra,
	};
}
function native(
	f: { path: string; c: string },
	request: Journal = base,
	key?: string,
	extra: Record<string, unknown> = {},
	helper = application,
	extraEnv: Record<string, string> = {},
) {
	const out = join(temp, `report-${serial++}.json`),
		p = Bun.spawnSync(args(helper), {
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
		createdBy: "user:u",
		createdByProgram: "rentemester-rust",
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
// Synthetic legacy-floor fixture only: restore exact canonical guards before native admission.
function syntheticStaleCounter(path: string) {
	const db = open(path);
	const guards = db
		.query(
			"SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name='sequences'",
		)
		.all() as { name: string; sql: string }[];
	for (const guard of guards)
		db.exec(`DROP TRIGGER "${guard.name.replaceAll('"', '""')}"`);
	db.exec("UPDATE sequences SET value=0");
	for (const guard of guards) db.exec(guard.sql);
	db.close(true);
}
function snapshot(path: string) {
	return encoded(tables(path));
}
function checked(id: string, f: () => void) {
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
function accepted(value: any) {
	assert.equal(value.outcome, "committed", encoded(value));
	assert.equal(value.result.ok, true, encoded(value));
	return value.result;
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
try {
	for (const mode of ["wal", "delete"]) {
		const positives: [string, Journal, string?][] = [
			["capital", base],
			["future-boundary", { ...base, transactionDate: "2026-02-15" }],
			[
				"dkk-optional-header",
				{
					...base,
					amountForeign: "3.00",
					amountDkk: "10.00",
					fxRateToDkk: "1.005000",
				},
			],
			[
				"bank-negative",
				{ ...base, sourceBankTransactionId: "-1" },
				"INSERT INTO bank_transactions(id,transaction_date,text,amount)VALUES(-1,'2026-01-01','Synthetic negative',10); INSERT INTO exceptions(type,message,related_bank_transaction_id)VALUES('UNMATCHED_BANK_TRANSACTION','Synthetic unchanged',-1)",
			],
			[
				"bank-zero",
				{ ...base, sourceBankTransactionId: "0" },
				"INSERT INTO bank_transactions(id,transaction_date,text,amount)VALUES(0,'2026-01-01','Synthetic zero',10); INSERT INTO exceptions(type,message,related_bank_transaction_id)VALUES('UNMATCHED_BANK_TRANSACTION','Synthetic unchanged',0)",
			],
			[
				"document",
				{
					...base,
					documentId: "1",
					lines: [
						{ accountNo: "3000", debitAmount: "10.00" },
						{ accountNo: "2000", creditAmount: "10.00" },
					],
				},
				documentSql,
			],
			[
				"manual-vat",
				{
					...base,
					documentId: "1",
					lines: [
						{
							accountNo: "3000",
							debitAmount: "100.00",
							vatCode: " \ufeffDK_PURCHASE_25 ",
						},
						{ accountNo: "4000", debitAmount: "25.00" },
						{ accountNo: "2000", creditAmount: "125.00" },
					],
				},
				documentSql,
			],
			[
				"fx",
				{
					...base,
					currency: "EUR",
					amountForeign: "10.00",
					amountDkk: "74.50",
					fxRateToDkk: "7.450000",
					lines: [
						{ accountNo: "2000", debitAmount: "74.50" },
						{ accountNo: "5000", creditAmount: "74.50" },
					],
				},
			],
			["bank-exceptions", { ...base, sourceBankTransactionId: "1" }, bankSql],
			[
				"unicode-empty-null",
				{
					...base,
					text: " \u0085Synthetic\nø😀\u0000 ",
					lines: [
						{ accountNo: "2000", debitAmount: "10.00", vatCode: " ", text: "" },
						{ accountNo: "5000", creditAmount: "10.00", text: null },
					],
				},
			],
			[
				"fiscal-span",
				{ ...base, transactionDate: "2025-10-01" },
				"INSERT INTO companies(name,cvr,fiscal_year_start_month,fiscal_year_label_strategy) VALUES('Synthetic',' dk11223344 ',7,'span')",
			],
			[
				"fiscal-start",
				{ ...base, transactionDate: "2025-10-01" },
				"INSERT INTO companies(name,fiscal_year_start_month,fiscal_year_label_strategy) VALUES('Synthetic',7,'start-year')",
			],
			[
				"retention-leap",
				{ ...base, transactionDate: "2027-02-28" },
				"INSERT INTO companies(name,fiscal_year_start_month) VALUES('Synthetic',3)",
			],
		];
		for (const [name, request, sql] of positives) {
			if (name === "retention-leap") request.transactionDate = "2024-02-29";
			checked(`${mode}/source-real-${name}`, () => {
				const f = fixture(mode, sql),
					legacy = join(temp, `source-${serial++}.sqlite`);
				copyFileSync(f.path, legacy);
				const l = source(legacy, request),
					n = accepted(native(f, request));
				assert.equal(l.ok, true, encoded(l));
				assert.deepEqual(n, l);
				assert.deepEqual(tables(f.path, true), tables(legacy, true));
				if (name === "capital") {
					const db = new Database(f.path);
					assert.equal(verifyAuditChain(db).ok, true);
					db.close(true);
				}
			});
		}
		checked(`${mode}/second-post-chains`, () => {
			const f = fixture(mode);
			const a = accepted(native(f)),
				b = accepted(native(f, { ...base, text: "Synthetic second" }));
			assert.notEqual(a.entryHash, b.entryHash);
			assert.equal(b.entryNo, "2026-00002");
			const db = new Database(f.path);
			assert.equal(verifyAuditChain(db).ok, true);
			db.close(true);
		});
		checked(`${mode}/wide-entry-number-floor`, () => {
			const f = fixture(mode),
				db = open(f.path);
			db.exec(
				"INSERT INTO sequences(kind,scope,value)VALUES('journal_entry','company-1:2026',99998)",
			);
			db.close(true);
			accepted(native(f));
			syntheticStaleCounter(f.path);
			assert.equal(accepted(native(f)).entryNo, "2026-100000");
			syntheticStaleCounter(f.path);
			assert.equal(accepted(native(f)).entryNo, "2026-100001");
		});
		checked(`${mode}/actual-post-receipt-replay-and-conflict`, () => {
			const f = fixture(mode),
				a = native(f, base, "key"),
				b = native(f, base, "key"),
				c = native(f, { ...base, text: "different" }, "key");
			accepted(a);
			accepted(b);
			assert.equal(a.receipt.replayed, false);
			assert.equal(b.receipt.replayed, true);
			assert.deepEqual(a.result, b.result);
			assert.equal(c.result.code, "IDEMPOTENCY_CONFLICT");
			assert.equal(count(f.path, "journal_entries"), 1);
			assert.equal(count(f.path, "journal_lines"), 2);
		});
		const negatives: [string, Journal, string?][] = [
			[
				"fx-amount",
				{
					...base,
					currency: "EUR",
					amountForeign: "10.00",
					amountDkk: "70.00",
					fxRateToDkk: "7.450000",
				},
			],
			[
				"zero-document",
				{
					...base,
					documentId: "0",
					lines: [
						{ accountNo: "3000", debitAmount: "10.00" },
						{ accountNo: "2000", creditAmount: "10.00" },
					],
				},
				"INSERT INTO documents(id,source,sha256_hash)VALUES(0,'synthetic','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb')",
			],
			["blank-text", { ...base, text: " " }],
			[
				"unbalanced",
				{
					...base,
					lines: [
						{ accountNo: "2000", debitAmount: "10.00" },
						{ accountNo: "5000", creditAmount: "9.00" },
					],
				},
			],
			[
				"missing-account",
				{
					...base,
					lines: [
						{ accountNo: "999999", debitAmount: "10.00" },
						{ accountNo: "5000", creditAmount: "10.00" },
					],
				},
			],
			[
				"inactive-account",
				base,
				"UPDATE accounts SET active=0 WHERE account_no='2000'",
			],
			[
				"negative-amount",
				{
					...base,
					lines: [
						{ accountNo: "2000", debitAmount: "-10.00" },
						{ accountNo: "5000", creditAmount: "-10.00" },
					],
				},
			],
			[
				"document-required",
				{
					...base,
					lines: [
						{ accountNo: "3000", debitAmount: "10.00" },
						{ accountNo: "2000", creditAmount: "10.00" },
					],
				},
			],
			[
				"document-missing",
				{
					...base,
					documentId: "42",
					lines: [
						{ accountNo: "3000", debitAmount: "10.00" },
						{ accountNo: "2000", creditAmount: "10.00" },
					],
				},
			],
			["bank-missing", { ...base, sourceBankTransactionId: "42" }],
			[
				"vat-code",
				{
					...base,
					lines: [{ ...base.lines[0], vatCode: "BAD" }, base.lines[1]],
				},
			],
			["fx-missing", { ...base, currency: "EUR" }],
			["future-date", { ...base, transactionDate: "2026-02-16" }],
			[
				"closed-period",
				base,
				"INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-01-01','2026-12-31','fiscal_year','closed')",
			],
		];
		for (const [name, request, sql] of negatives)
			checked(`${mode}/refusal-${name}`, () => {
				const f = fixture(mode, sql),
					legacy = join(temp, `negative-${serial++}.sqlite`);
				copyFileSync(f.path, legacy);
				const before = snapshot(f.path),
					n = native(f, request, "retry"),
					l = source(legacy, request);
				assert.equal(l.ok, false, encoded(l));
				assert.equal(n.outcome, "not_committed", encoded(n));
				assert.deepEqual(n.business, l);
				assert.equal(snapshot(f.path), before);
				assert.equal(snapshot(legacy), before);
			});
		checked(`${mode}/optional-balance-document-actual-fk`, () => {
			const f = fixture(mode),
				before = snapshot(f.path);
			const n = native(f, { ...base, documentId: "99" }, "bad-fk");
			assert.equal(n.outcome, "not_committed");
			assert.equal(n.code, "posting_storage_failure");
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/direct-bank-source-actual-unique-index`, () => {
			const f = fixture(mode, bankSql);
			accepted(native(f, { ...base, sourceBankTransactionId: "1" }));
			const before = snapshot(f.path);
			assert.equal(
				native(f, { ...base, sourceBankTransactionId: "1" }, "duplicate")
					.outcome,
				"not_committed",
			);
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/closed-reopened-period`, () => {
			const f = fixture(
				mode,
				"INSERT INTO accounting_periods(id,period_start,period_end,kind,status)VALUES(1,'2026-01-01','2026-12-31','fiscal_year','closed'); INSERT INTO audit_log(event_type,entity_type,entity_id,message)VALUES('period_reopen','accounting_period','1','Synthetic reopen')",
			);
			accepted(native(f));
		});
		checked(`${mode}/native-precision-refusal`, () => {
			const f = fixture(mode),
				before = snapshot(f.path),
				request = {
					...base,
					lines: [
						{ accountNo: "2000", debitAmount: "9007199254740992.00" },
						{ accountNo: "5000", creditAmount: "9007199254740992.00" },
					],
				};
			assert.equal(
				native(f, request, "precision").code,
				"unsupported_legacy_number",
			);
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/legacy-fx-multiplication-refusal`, () => {
			const f = fixture(mode),
				before = snapshot(f.path),
				request = {
					...base,
					currency: "EUR",
					amountForeign: "3.00",
					amountDkk: "3.02",
					fxRateToDkk: "1.005000",
					lines: [
						{ accountNo: "2000", debitAmount: "3.02" },
						{ accountNo: "5000", creditAmount: "3.02" },
					],
				};
			const n = native(f, request, "ambiguous");
			assert.equal(n.code, "unsupported_legacy_number");
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/receipt-storage-failure-rolls-back-real-post`, () => {
			const f = fixture(
					mode,
					`INSERT INTO mutation_idempotency_tombstones(id,client_key_hash,ledger_uuid,operation,principal_kind,principal_subject_id,payload_hash,created_at,expires_at)SELECT 9007199254740991,'${"0".repeat(64)}',ledger_uuid,'journal_post','user','other','${"a".repeat(64)}','2026-01-01T00:00:00.000Z','2026-01-31T00:00:00.000Z' FROM ledger_identity`,
				),
				before = snapshot(f.path);
			assert.equal(native(f, base, "new").code, "IDEMPOTENCY_STORAGE_FAILURE");
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/schema-drift-hides-committed-receipt`, () => {
			const f = fixture(mode);
			accepted(native(f, base, "drift"));
			const db = open(f.path);
			db.exec("DROP TRIGGER journal_entries_no_update");
			db.close(true);
			const before = snapshot(f.path),
				n = native(f, base, "drift");
			assert.equal(n.code, "unsupported_schema");
			assert.equal(n.result, undefined);
			assert.equal(n.receipt, undefined);
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/credit-note-duplicate-matches-source`, () => {
			const f = fixture(
					mode,
					"INSERT INTO documents(id,source,sha256_hash,document_type)VALUES(1,'synthetic','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc','credit_note')",
				),
				request = { ...base, documentId: "1" };
			accepted(native(f, request));
			const legacy = join(temp, `credit-${serial++}.sqlite`),
				checkpoint = open(f.path);
			checkpoint.query("PRAGMA wal_checkpoint(TRUNCATE)").all();
			checkpoint.close(true);
			copyFileSync(f.path, legacy);
			const before = snapshot(f.path),
				n = native(f, request, "another"),
				l = source(legacy, request);
			assert.equal(n.outcome, "not_committed");
			assert.deepEqual(n.business, l);
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/reported-period-is-terminal`, () => {
			const f = fixture(
					mode,
					"INSERT INTO accounting_periods(id,period_start,period_end,kind,status)VALUES(1,'2026-01-01','2026-12-31','fiscal_year','reported'); INSERT INTO audit_log(event_type,entity_type,entity_id,message)VALUES('period_reopen','accounting_period','1','Synthetic ineffective reopen')",
				),
				before = snapshot(f.path);
			assert.equal(native(f).outcome, "not_committed");
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/wrong-rule-version`, () => {
			const f = fixture(mode),
				before = snapshot(f.path);
			assert.equal(
				native(f, base, undefined, {
					input: { ...input(), expectedRuleVersion: "unknown" },
				}).outcome,
				"not_committed",
			);
			assert.equal(snapshot(f.path), before);
		});
		checked(`${mode}/current-revocation-hides-real-receipt`, () => {
			const f = fixture(mode);
			accepted(native(f, base, "revoked"));
			const db = open(f.c);
			db.exec(
				"INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'agent:synthetic')",
			);
			db.close(true);
			const before = snapshot(f.path);
			const n = native(f, base, "revoked");
			assert.equal(n.outcome, "not_committed");
			assert.equal(n.code, "access_denied");
			assert.equal(n.result, undefined);
			assert.equal(snapshot(f.path), before);
		});
		for (const [name, extra] of Object.entries({
			archived: { archived: true },
			unregistered: { registered: false },
			wrong_workspace: { workspace: "other" },
			mixed: { service: "rms_synthetic" },
		}))
			checked(`${mode}/authority-${name}`, () => {
				const f = fixture(mode),
					before = snapshot(f.path);
				assert.equal(
					native(f, base, undefined, extra).outcome,
					"not_committed",
				);
				assert.equal(snapshot(f.path), before);
			});
		for (const point of [
			"sequence",
			"header",
			"line-0",
			"line-1",
			"lines",
			"audit",
			"exceptions",
		]) {
			checked(`${mode}/late-sql-${point}`, () => {
				const f = fixture(mode, bankSql),
					before = snapshot(f.path);
				assert.equal(
					native(
						f,
						{ ...base, sourceBankTransactionId: "1" },
						undefined,
						{},
						storage,
						{ RM_POST_FAIL: point },
					).outcome,
					"not_committed",
				);
				assert.equal(snapshot(f.path), before);
			});
			const f = fixture(mode, bankSql),
				before = snapshot(f.path),
				marker = join(temp, `kill-${serial++}`),
				out = marker + ".json";
			const child = Bun.spawn(args(storage), {
				cwd: root,
				env: processEnv(
					f,
					config({ ...base, sourceBankTransactionId: "1" }),
					out,
					{ RM_POST_POINT: point, RM_POST_MARKER: marker },
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
				assert.equal(snapshot(f.path), before);
				assert.equal(accepted(native(f)).entryNo, "2026-00001");
			});
		}
		for (const point of ["precommit", "postcommit"]) {
			const f = fixture(mode),
				marker = join(temp, `fence-${serial++}`),
				out = marker + ".json";
			const child = Bun.spawn(args(application), {
				cwd: root,
				env: processEnv(f, config(base, "lost"), out, {
					RM_FENCE_POINT: point,
					RM_FENCE_MARKER: marker,
				}),
				stdin: "pipe",
				stdout: "pipe",
				stderr: "pipe",
			});
			await waitFor(marker);
			child.kill("SIGKILL");
			await child.exited;
			checked(`${mode}/death-real-receipt-${point}`, () => {
				assert.equal(existsSync(out), false);
				assert.equal(
					count(f.path, "journal_entries"),
					point === "precommit" ? 0 : 1,
				);
				const retry = native(f, base, "lost");
				accepted(retry);
				assert.equal(retry.receipt.replayed, point === "postcommit");
				assert.equal(count(f.path, "journal_entries"), 1);
			});
		}
		for (const concurrentKey of [undefined, "same-key"]) {
			const f = fixture(mode),
				marker = join(temp, `concurrent-${serial++}`),
				outA = marker + "a.json",
				outB = marker + "b.json";
			const a = Bun.spawn(args(application), {
				cwd: root,
				env: processEnv(
					f,
					config(base, concurrentKey, { busyMs: 10000 }),
					outA,
					{
						RM_FENCE_POINT: "precommit",
						RM_FENCE_MARKER: marker,
					},
				),
				stdin: "pipe",
				stdout: "pipe",
				stderr: "pipe",
			});
			await waitFor(marker);
			const b = Bun.spawn(args(application), {
				cwd: root,
				env: processEnv(
					f,
					config(
						{ ...base, text: concurrentKey ? base.text : "Concurrent second" },
						concurrentKey,
						{
							busyMs: 10000,
						},
					),
					outB,
				),
				stdout: "pipe",
				stderr: "pipe",
			});
			await Bun.sleep(50);
			assert.equal(b.exitCode, null);
			a.stdin.write("continue\n");
			a.stdin.end();
			assert.equal(await a.exited, 0, await new Response(a.stderr).text());
			assert.equal(await b.exited, 0, await new Response(b.stderr).text());
			checked(
				`${mode}/${concurrentKey ? "concurrent-same-key" : "actual-concurrent-posters"}`,
				() => {
					const first = accepted(JSON.parse(readFileSync(outA, "utf8"))),
						second = accepted(JSON.parse(readFileSync(outB, "utf8")));
					assert.equal(first.entryNo, "2026-00001");
					assert.equal(
						second.entryNo,
						concurrentKey ? "2026-00001" : "2026-00002",
					);
					if (concurrentKey) {
						const secondReply = JSON.parse(readFileSync(outB, "utf8"));
						assert.equal(secondReply.receipt.replayed, true);
						assert.equal(count(f.path, "journal_entries"), 1);
					}
					const db = new Database(f.path);
					assert.equal(verifyAuditChain(db).ok, true);
					db.close(true);
				},
			);
		}
	}
	checked("actual-delete-post-commit-unknown", () => {
		const f = fixture("delete"),
			reader = open(f.path);
		reader.exec("BEGIN");
		reader.query("SELECT * FROM ledger_identity").all();
		const n = native(f, base, "unknown");
		assert.equal(n.outcome, "unknown", encoded(n));
		assert.equal(n.companyReusable, false);
		assert.equal(n.result, undefined);
		assert.equal(n.receipt, undefined);
		reader.exec("ROLLBACK");
		reader.close(true);
	});
	console.log(
		JSON.stringify(
			{
				ok: true,
				scope:
					"actual native manual journal post/source accounting parity and authenticated admitted-connection process faults; no deployed original writer authority or reversal",
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
