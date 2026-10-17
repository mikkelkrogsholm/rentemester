import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
import {
	retainUntilForDate,
	backfillRetentionDeadlines,
} from "../../src/core/retention";
test("retention computes valid target calendar deadline", () => {
	const db = new Database(":memory:");
	migrate(db);
	db.exec(
		"INSERT INTO companies(id,name,fiscal_year_start_month) VALUES(1,'Synthetic',3)",
	);
	expect(retainUntilForDate(db, "2024-02-29")).toBe("2029-02-28");
	db.exec("UPDATE companies SET fiscal_year_start_month=1");
	expect(retainUntilForDate(db, " 2026-01-01 ")).toBe("2031-12-31");
	expect(retainUntilForDate(db, "0001-07-01")).toBe("0006-12-31");
	expect(() => retainUntilForDate(db, "9999-01-01")).toThrow();
	db.close();
});

test("retention updates exact SQLite IDs beyond JavaScript safe integers", () => {
	const db = new Database(":memory:");
	migrate(db);
	db.exec(
		"INSERT INTO documents(id,source,sha256_hash,invoice_date) VALUES(9007199254740992,'synthetic','large-a','2026-01-01'),(9007199254740993,'synthetic','large-b','2027-01-01')",
	);
	backfillRetentionDeadlines(db);
	expect(
		db
			.query(
				"SELECT CAST(id AS TEXT) AS id,retain_until FROM documents ORDER BY id",
			)
			.all(),
	).toEqual([
		{ id: "9007199254740992", retain_until: "2031-12-31" },
		{ id: "9007199254740993", retain_until: "2032-12-31" },
	]);
	db.exec(
		"UPDATE documents SET retain_until='existing-preserved' WHERE id=9007199254740992;UPDATE documents SET retain_until=NULL WHERE id=9007199254740993",
	);
	backfillRetentionDeadlines(db);
	expect(
		db
			.query(
				"SELECT CAST(id AS TEXT) AS id,retain_until FROM documents ORDER BY id",
			)
			.all(),
	).toEqual([
		{ id: "9007199254740992", retain_until: "existing-preserved" },
		{ id: "9007199254740993", retain_until: "2032-12-31" },
	]);
	db.close();
});
