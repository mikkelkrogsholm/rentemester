import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
test("canonical VAT normalization preserves current close-decision foreign keys and guards", () => {
	const db = new Database(":memory:");
	db.exec("PRAGMA foreign_keys=ON");
	migrate(db);
	db.exec(
		"INSERT INTO companies(id,name,vat_period_type) VALUES(1,'Synthetic','quarter');INSERT INTO accounting_periods(id,period_start,period_end,kind,status,reference,created_at) VALUES(9007199254740993,'2027-01-01','2027-03-31','vat_quarter','closed','Synthetic','2020-01-01');INSERT INTO period_close_readiness_packets(id,packet_hash,period_start,period_end,cutoff,packet_json) VALUES(1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','2027-01-01','2027-03-31','2027-04-01','{}');INSERT INTO period_close_decisions(id,period_id,packet_hash,decision,actor) VALUES(1,9007199254740993,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','closed','agent:synthetic');INSERT INTO period_close_open_items(id,decision_id,period_id,packet_hash,code,severity,count,amount,evidence_json,reason,actor) VALUES(1,1,9007199254740993,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','synthetic','warning',1,10,'{}','synthetic','agent:synthetic')",
	);
	const schema = () =>
		db
			.query(
				"SELECT name,sql FROM sqlite_schema WHERE substr(name,1,7)<>'sqlite_' ORDER BY name",
			)
			.all();
	const beforeSchema = schema();
	const children = db.query("SELECT * FROM period_close_open_items").all();
	const history = db.query("SELECT * FROM schema_migrations").all();
	expect(() => migrate(db)).not.toThrow();
	expect(
		db
			.query(
				"SELECT CAST(id AS TEXT) id,kind,reference,created_at FROM accounting_periods",
			)
			.get(),
	).toEqual({
		id: "9007199254740993",
		kind: "vat_period",
		reference: "Synthetic",
		created_at: "2020-01-01",
	});
	expect(schema()).toEqual(beforeSchema);
	expect(db.query("SELECT * FROM period_close_open_items").all()).toEqual(
		children,
	);
	expect(db.query("SELECT * FROM schema_migrations").all()).toEqual(history);
	expect(db.query("PRAGMA foreign_key_check").all()).toEqual([]);
	expect(() =>
		db.exec("UPDATE accounting_periods SET kind='vat_quarter'"),
	).toThrow();
	expect(() => db.exec("DELETE FROM accounting_periods")).toThrow();
	db.close();
});
