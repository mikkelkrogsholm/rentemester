import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { migrate } from "../../src/core/db";

test("routine migration keeps legacy started purchase applications immutable", () => {
  const db = new Database(":memory:");
  try {
    migrate(db);
    db.exec("INSERT INTO documents(id,source,sha256_hash) VALUES(1,'synthetic','synthetic-application');INSERT INTO purchase_posting_applications(id,document_id,application_key,status) VALUES(1,1,'synthetic-original','started')");
    for (let open=0;open<2;open++) {
      migrate(db);
      expect(() => db.exec("UPDATE purchase_posting_applications SET application_key='synthetic-tampered' WHERE id=1")).toThrow("append-only");
      expect(() => db.exec("UPDATE purchase_posting_applications SET status='posted' WHERE id=1")).toThrow("append-only");
      expect(() => db.exec("DELETE FROM purchase_posting_applications WHERE id=1")).toThrow("append-only");
    }
    expect(db.query("SELECT application_key,status FROM purchase_posting_applications WHERE id=1").get()).toEqual({application_key:"synthetic-original",status:"started"});
  } finally { db.close(); }
});
