import { test, expect } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { mkdirSync } from "node:fs";
import { assertLegacySourceActive } from "../../src/core/legacy-retirement";
import { openDb } from "../../src/core/db";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { saveWorkspaceManifest } from "../../src/core/workspace";

test("retired company and control opens refuse before creating sidecars or running migrations", () => {
  const root=mkdtempSync(join(tmpdir(),"rm-retired-synthetic-"));
  try {
    const paths=[join(root,"ledger.sqlite"),join(root,".rentemester/workspace-control.sqlite")];
    mkdirSync(join(root,".rentemester"));
    for(const path of paths){
      const db=new Database(path);db.exec("CREATE TABLE rm_legacy_retirement_v1(id INTEGER PRIMARY KEY);INSERT INTO rm_legacy_retirement_v1 VALUES(1)");db.close();
    }
    mkdirSync(join(root,"new-company","data"),{recursive:true});
    expect(()=>openDb(join(root,"new-company","data","ledger.sqlite"))).toThrow("legacy source retired");
    expect(()=>saveWorkspaceManifest(root,{version:1,companies:[]})).toThrow("legacy source retired");
    const before=paths.map(path=>Buffer.from(readFileSync(path)));
    expect(()=>openDb(paths[0]!)).toThrow("legacy source retired for Rust adoption");
    expect(()=>openWorkspaceControlDb(root)).toThrow("legacy source retired for Rust adoption");
    paths.forEach((path,index)=>{ expect(readFileSync(path).equals(before[index]!)).toBe(true); });
  } finally {rmSync(root,{recursive:true,force:true});}
});

test("even a partial retirement trigger fails closed while unfenced reads remain available",()=>{
  const db=new Database(":memory:");
  try {
    db.exec("CREATE TABLE business(value TEXT);INSERT INTO business VALUES('historical')");
    expect(()=>assertLegacySourceActive(db)).not.toThrow();
    db.exec("CREATE TRIGGER rm_legacy_retired_partial BEFORE INSERT ON business BEGIN SELECT RAISE(ABORT,'legacy source retired for Rust adoption'); END");
    expect(()=>assertLegacySourceActive(db)).toThrow("legacy source retired for Rust adoption");
    expect(db.query("SELECT value FROM business").get()).toEqual({value:"historical"});
    expect(()=>db.exec("INSERT INTO business VALUES('changed')")).toThrow("legacy source retired");
  } finally {db.close();}
});
