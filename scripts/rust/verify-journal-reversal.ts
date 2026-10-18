/** Actual source/native append-only reversals on canonical synthetic company/control databases. */
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
import { asJournalEntryId } from "../../src/core/ids";
import { postDineroPostings } from "../../src/core/import/dinero-postings";
import { migrate } from "../../src/core/db";
import {
	postJournalEntryInCurrentTransaction,
 reverseJournalEntryInCurrentTransaction,
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
	temp = mkdtempSync(join(tmpdir(), "rm-real-reversal-"));
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
type Reverse = { entryId: string; transactionDate: string; reason: string };
const correction: Reverse = {entryId: "1", transactionDate: "2026-01-02", reason: " Synthetic correction "};
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
function input(request: Reverse = correction, key?: string) {
	return {
		format: "rentemester-manual-journal-reversal/1",
		expectedRuleVersion: version,
		...(key ? { idempotencyKey: key } : {}),
		request,
	};
}
function config(
	request: Reverse,
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
		? "journal_reversal::tests::child"
		: "journal_reversal::tests::child",
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
	request: Reverse = correction,
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
function seed(path: string, request: Journal = base) {
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
function source(path: string, request: Reverse = correction) {
 const db=new Database(path); db.exec("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL");
 try { return db.transaction(()=>reverseJournalEntryInCurrentTransaction(db,{entryId:asJournalEntryId(Number(request.entryId)),transactionDate:request.transactionDate,reason:request.reason,createdBy:"user:u",createdByProgram:"rentemester-rust"})).immediate(); }
 finally { db.close(true); }
}
function prepared(mode: string, original: Journal=base, sql="") {
 const f=fixture(mode,sql); const result=seed(f.path,original); assert.equal(result.ok,true,encoded(result)); return f;
}
function mutate(path:string, table:string, sql:string) {
 const db=open(path); const guards=db.query("SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name=?").all(table) as {name:string;sql:string}[];
 for(const guard of guards) db.exec(`DROP TRIGGER "${guard.name.replaceAll('"','""')}"`);
 db.exec(sql); for(const guard of guards) db.exec(guard.sql); db.close(true);
}
function oracle(f:{path:string;c:string}) {
 const db=open(f.path); db.query("PRAGMA wal_checkpoint(TRUNCATE)").all();db.close(true);
 const p=join(temp,`oracle-${serial++}.sqlite`); copyFileSync(f.path,p);return p;
}
function originals(path:string) {
 const db=open(path);try {return encoded([db.query("SELECT * FROM journal_entries WHERE id=1").all(),db.query("SELECT * FROM journal_lines WHERE journal_entry_id=1 ORDER BY id").all()]);}finally{db.close(true);}
}
try {
 for(const mode of ["wal","delete"]) {
  const positives:[string,Journal,string?][]=[
   ["capital",base],
   ["raw-text",{...base,lines:[{accountNo:"2000",debitAmount:"10.00",text:"\0 Ø😀 \u0085"},{accountNo:"5000",creditAmount:"10.00",text:""}]}],
   ["document",{...base,documentId:"1",lines:[{accountNo:"3000",debitAmount:"10.00",vatCode:"DK_PURCHASE_25"},{accountNo:"2000",creditAmount:"10.00"}]},documentSql],
   ["bank-unchanged",{...base,sourceBankTransactionId:"1"},bankSql],
   ["fx",{...base,currency:"EUR",amountForeign:"10.00",amountDkk:"74.50",fxRateToDkk:"7.450000",lines:[{accountNo:"2000",debitAmount:"74.50"},{accountNo:"5000",creditAmount:"74.50"}]}],
   ["dkk-optional-fx",{...base,fxRateToDkk:"1.005000"}],
   ["dkk-zero-fx",{...base,fxRateToDkk:"0.000000"}],
   ["dkk-negative-fx",{...base,fxRateToDkk:"-1.000000"}],
   ["fiscal-span",base,"INSERT INTO companies(name,cvr,fiscal_year_start_month,fiscal_year_label_strategy)VALUES('Synthetic','DK11223344',7,'span')"],
  ];
  for(const [name,original,sql] of positives) checked(`${mode}/source-${name}`,()=>{
   const f=prepared(mode,original,sql),before=originals(f.path),p=oracle(f),expected=source(p),actual=accepted(native(f));
   assert.deepEqual(actual,expected);assert.equal(encoded(tables(f.path,true)),encoded(tables(p,true)));assert.equal(originals(f.path),before);
   // Complete synthetic capital ledgers have the evidence required by source full audit.
   if(["capital","raw-text","bank-unchanged","fx","dkk-optional-fx","fiscal-span"].includes(name)){const db=new Database(f.path);assert.equal(verifyAuditChain(db).ok,true, name);db.close(true);}
  });
  // Persisted historical evidence may predate public manual restrictions. Restore
  // exact canonical DDL before either implementation is admitted; compare both.
  for(const variant of ["historical-expense","historical-with-document","fake-import","uncoded-vat","raw-vat-empty","raw-vat-space","inactive","unsupported-vat","protected-invoice","protected-credit-note"]) checked(`${mode}/${variant}`,()=>{
   const f=prepared(mode,base,documentSql);
   if(variant.startsWith("historical")||variant==="fake-import"){
    mutate(f.path,"journal_lines","UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE account_no='3000') WHERE id=1");
    mutate(f.path,"journal_entries",`UPDATE journal_entries SET created_by_program='${variant==="fake-import"?" rentemester-import-postings":"rentemester-import-postings"}'${variant==="historical-with-document"?",document_id=1":""} WHERE id=1`);
   }
   if(variant==="uncoded-vat") mutate(f.path,"journal_lines","UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE account_no='4000') WHERE id=1");
   if(variant.startsWith("raw-vat")) mutate(f.path,"journal_lines",`UPDATE journal_lines SET vat_code='${variant.endsWith("space")?" ":""}' WHERE id=1`);
   if(variant==="inactive"){const db=open(f.path);db.exec("UPDATE accounts SET active=0 WHERE account_no='2000'");db.close(true);}
   if(variant==="unsupported-vat")mutate(f.path,"journal_lines","UPDATE journal_lines SET vat_code='UNSUPPORTED' WHERE id=1");
   if(variant.startsWith("protected")){
    mutate(f.path,"documents",`UPDATE documents SET document_type='${variant.endsWith("invoice")?"issued_invoice":"credit_note"}' WHERE id=1`);
    mutate(f.path,"journal_entries","UPDATE journal_entries SET document_id=1 WHERE id=1");
   }
   const p=oracle(f),before=snapshot(f.path),original=originals(f.path),expected=source(p),actual=native(f);
   assert.deepEqual(expected.ok?accepted(actual):actual.business,expected);
   assert.equal(encoded(tables(f.path,true)),encoded(tables(p,true)));assert.equal(originals(f.path),original);
   if(!expected.ok)assert.equal(snapshot(f.path),before);
  });
  for(const [name,request] of [["missing-original",{...correction,entryId:"999"}],["blank-reason",{...correction,reason:"\ufeff \t"}],["future",{...correction,transactionDate:"2026-02-16"}]] as [string,Reverse][])checked(`${mode}/${name}`,()=>{
   const f=prepared(mode),p=oracle(f),before=snapshot(f.path);assert.deepEqual(native(f,request).business,source(p,request));assert.equal(snapshot(f.path),before);
  });
  checked(`${mode}/single-reversal-and-reversal-of-reversal`,()=>{
   const f=prepared(mode),p=oracle(f);assert.deepEqual(accepted(native(f)),source(p));
   for(const request of [correction,{...correction,entryId:"2"}]){const before=snapshot(f.path);assert.deepEqual(native(f,request).business,source(p,request));assert.equal(snapshot(f.path),before);}
  });
  checked(`${mode}/receipt-replay-conflict`,()=>{
   const f=prepared(mode),result=accepted(native(f,correction,"key")),before=originals(f.path);
   const replay=native(f,correction,"key");assert.deepEqual(accepted(replay),result);assert.equal(replay.receipt.replayed,true);
   assert.equal(native(f,{...correction,reason:"changed"},"key").result.ok,false);assert.equal(count(f.path,"journal_entries"),2);assert.equal(originals(f.path),before);
  });
  checked(`${mode}/receipt-revoked-current-membership`,()=>{
   const f=prepared(mode);accepted(native(f,correction,"revoked"));const db=open(f.c);db.exec("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'agent:synthetic')");db.close(true);
   const before=snapshot(f.path),n=native(f,correction,"revoked");assert.equal(n.code,"access_denied");assert.equal(n.result,undefined);assert.equal(snapshot(f.path),before);
  });
  checked(`${mode}/actual-source-historical-import`,()=>{
   const f=fixture(mode),db=new Database(f.path);
   const result=postDineroPostings(db,[{transactionDate:"2026-01-01",text:"Synthetic historic expense",voucherRef:"SYN-1",lines:[{accountNo:"3000",debitAmount:10,text:"Imported expense"},{accountNo:"2000",creditAmount:10,text:"Bank"}]}],new Set(["3000","2000"]),{createdBy:"user:u"});assert.equal(result.ok,true,encoded(result));db.close(true);
   const before=originals(f.path),p=oracle(f);assert.deepEqual(accepted(native(f)),source(p));assert.equal(encoded(tables(f.path,true)),encoded(tables(p,true)));assert.equal(originals(f.path),before);
   const verify=new Database(f.path);assert.equal(verifyAuditChain(verify).ok,true);verify.close(true);
  });
  for(const state of ["closed","reported","reopened"])checked(`${mode}/period-${state}`,()=>{
   const f=prepared(mode),db=open(f.path);db.exec(`INSERT INTO accounting_periods(id,period_start,period_end,kind,status)VALUES(1,'2026-01-02','2026-12-31','fiscal_year','${state==="reported"?"reported":"closed"}')`);
   if(state==="reopened")db.exec("INSERT INTO audit_log(event_type,entity_type,entity_id,message)VALUES('period_reopen','accounting_period','1','Synthetic reopen')");db.close(true);
   const p=oracle(f),before=snapshot(f.path),expected=source(p),actual=native(f);assert.deepEqual(expected.ok?accepted(actual):actual.business,expected);if(!expected.ok)assert.equal(snapshot(f.path),before);else assert.equal(encoded(tables(f.path,true)),encoded(tables(p,true)));
  });
  checked(`${mode}/no-lines`,()=>{const f=prepared(mode);mutate(f.path,"journal_lines","DELETE FROM journal_lines");const p=oracle(f),before=snapshot(f.path);assert.deepEqual(native(f).business,source(p));assert.equal(snapshot(f.path),before);});
  for(const [name,table,sql] of [
   ["malformed-current-tail","journal_entries","UPDATE journal_entries SET entry_hash='GENESIS' WHERE id=1"],
   ["subcent-original","journal_lines","UPDATE journal_lines SET debit_amount=10.001 WHERE id=1"],
   ["unsafe-original-number","journal_lines","UPDATE journal_lines SET debit_amount=9007199254740992 WHERE id=1"],
  ])checked(`${mode}/${name}`,()=>{const f=prepared(mode);mutate(f.path,table,sql);const before=snapshot(f.path);assert.equal(native(f).outcome,"not_committed");assert.equal(snapshot(f.path),before);});
  checked(`${mode}/schema-before-receipt`,()=>{const f=prepared(mode);accepted(native(f,correction,"drift"));const db=open(f.path);db.exec("DROP TRIGGER journal_entries_no_update");db.close(true);const before=snapshot(f.path),n=native(f,correction,"drift");assert.equal(n.code,"unsupported_schema");assert.equal(n.result,undefined);assert.equal(snapshot(f.path),before);});
  checked(`${mode}/receipt-write-failure-undoes-real-reversal`,()=>{const f=prepared(mode),db=open(f.path);db.exec(`INSERT INTO mutation_idempotency_tombstones(id,client_key_hash,ledger_uuid,operation,principal_kind,principal_subject_id,payload_hash,created_at,expires_at)SELECT 9007199254740991,'${"0".repeat(64)}',ledger_uuid,'journal_reverse','user','other','${"a".repeat(64)}','2026-01-01T00:00:00.000Z','2026-01-31T00:00:00.000Z' FROM ledger_identity`);db.close(true);const before=snapshot(f.path),n=native(f,correction,"new");assert.equal(n.code,"IDEMPOTENCY_STORAGE_FAILURE");assert.equal(snapshot(f.path),before);});
  for(const [name,extra]of Object.entries({archived:{archived:true},unregistered:{registered:false},wrong_workspace:{workspace:"other"},mixed:{service:"rms_synthetic"}}))checked(`${mode}/authority-${name}`,()=>{const f=prepared(mode),before=snapshot(f.path);assert.equal(native(f,correction,undefined,extra).outcome,"not_committed");assert.equal(snapshot(f.path),before);});
  for(const point of ["sequence","header","line-0","line-1","lines","audit"]){
   checked(`${mode}/late-fault-${point}`,()=>{const f=prepared(mode),before=snapshot(f.path);assert.equal(native(f,correction,undefined,{},storage,{RM_POST_FAIL:point}).outcome,"not_committed");assert.equal(snapshot(f.path),before);assert.equal(accepted(native(f)).entryNo,"2026-00002");});
   const f=prepared(mode),before=snapshot(f.path),marker=join(temp,`death-${serial++}`),out=marker+".json";
   const child=Bun.spawn(args(storage),{cwd:root,env:processEnv(f,config(correction),out,{RM_POST_POINT:point,RM_POST_MARKER:marker}),stdin:"pipe",stdout:"pipe",stderr:"pipe"});await waitFor(marker);child.kill("SIGKILL");await child.exited;
   checked(`${mode}/death-${point}`,()=>{assert.equal(existsSync(out),false);assert.equal(snapshot(f.path),before);assert.equal(accepted(native(f)).entryNo,"2026-00002");});
  }
  for(const point of ["precommit","postcommit"]){
   const f=prepared(mode),marker=join(temp,`receipt-death-${serial++}`),out=marker+".json";
   const child=Bun.spawn(args(application),{cwd:root,env:processEnv(f,config(correction,"lost"),out,{RM_FENCE_POINT:point,RM_FENCE_MARKER:marker}),stdin:"pipe",stdout:"pipe",stderr:"pipe"});await waitFor(marker);child.kill("SIGKILL");await child.exited;
   checked(`${mode}/receipt-death-${point}`,()=>{assert.equal(existsSync(out),false);assert.equal(count(f.path,"journal_entries"),point==="precommit"?1:2);const n=native(f,correction,"lost");accepted(n);assert.equal(n.receipt.replayed,point==="postcommit");assert.equal(count(f.path,"journal_entries"),2);});
  }
  for(const key of [undefined,"same-key"]){
   const f=prepared(mode),marker=join(temp,`race-${serial++}`),outA=marker+"a",outB=marker+"b";
   const a=Bun.spawn(args(application),{cwd:root,env:processEnv(f,config(correction,key,{busyMs:10000}),outA,{RM_FENCE_POINT:"precommit",RM_FENCE_MARKER:marker}),stdin:"pipe",stdout:"pipe",stderr:"pipe"});await waitFor(marker);
   const b=Bun.spawn(args(application),{cwd:root,env:processEnv(f,config(correction,key,{busyMs:10000}),outB),stdout:"pipe",stderr:"pipe"});await Bun.sleep(50);assert.equal(b.exitCode,null);a.stdin.write("continue\n");a.stdin.end();assert.equal(await a.exited,0);assert.equal(await b.exited,0);
   checked(`${mode}/concurrent-${key??"same-original"}`,()=>{accepted(JSON.parse(readFileSync(outA,"utf8")));const n=JSON.parse(readFileSync(outB,"utf8"));if(key){accepted(n);assert.equal(n.receipt.replayed,true);}else{assert.equal(n.business.ok,false);assert.match(n.business.errors[0],/already has reversal/);}assert.equal(count(f.path,"journal_entries"),2);const db=new Database(f.path);assert.equal(verifyAuditChain(db).ok,true);db.close(true);});
  }
 }
 checked("actual-delete-reversal-commit-unknown",()=>{const f=prepared("delete"),reader=open(f.path);reader.exec("BEGIN");reader.query("SELECT * FROM ledger_identity").all();const n=native(f,correction,"unknown");assert.equal(n.outcome,"unknown",encoded(n));assert.equal(n.companyReusable,false);assert.equal(n.result,undefined);reader.exec("ROLLBACK");reader.close(true);});
 console.log(JSON.stringify({ok:true,scope:"actual source/native append-only manual reversal, original preservation, current human authority, receipt and process atomicity on admitted canonical connections",checks:observations.length,observations},null,2));
}finally{
 if(oldToday===undefined)delete process.env.RENTEMESTER_TODAY;else process.env.RENTEMESTER_TODAY=oldToday;
 if(oldFuture===undefined)delete process.env.RENTEMESTER_MAX_FUTURE_DAYS;else process.env.RENTEMESTER_MAX_FUTURE_DAYS=oldFuture;
 rmSync(temp,{recursive:true,force:true});
}
