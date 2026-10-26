import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { createKnowledgePage, createKnowledgeRelation, listKnowledgeBacklinks, listKnowledgePageHistory, listKnowledgePages, listKnowledgeRelationHistory, listKnowledgeRelations, readKnowledgeMetric, renderKnowledgeMarkdown, safeUrl, supersedeKnowledgePage, supersedeKnowledgeRelation } from "../../src/core/knowledge-pages";

const page = (scope: { kind: "workspace" } | { kind: "company"; companySlug: string }, pageId: string, slug: string, bodyMarkdown = "[[other-page]] https://example.test/reference") => ({ pageId, scope, slug, title: slug, bodyMarkdown, provenance: { kind: "external_snapshot" as const, ref: "https://example.test/source" }, effectiveFrom: "2026-01-01", actor: "user:test", principal: "user:test" });

describe("knowledge pages", () => {
  test("isolates scopes and rejects invalid scoped endpoints", () => {
    const root = mkdtempSync(join(tmpdir(), "rm-knowledge-pages-"));
    try {
      initWorkspace(root); const db = openWorkspaceControlDb(root);
      createKnowledgePage(db, page({ kind: "workspace" }, "page-workspace", "shared"));
      createKnowledgePage(db, page({ kind: "company", companySlug: "alpha-company" }, "page-alpha", "shared"));
      expect(listKnowledgePages(db, { scope: { kind: "workspace" } })).toHaveLength(1);
      expect(listKnowledgePages(db, { scope: { kind: "company", companySlug: "alpha-company" } })).toHaveLength(1);
      expect(() => createKnowledgeRelation(db, { relationId: "relation-invalid-page", scope: { kind: "workspace" }, type: "reference", subject: { kind: "page", ref: "page-workspace" }, object: { kind: "page", ref: "page-alpha" }, provenance: { kind: "user", ref: "test" }, effectiveFrom: "2026-01-01", actor: "user:test", principal: "user:test" })).toThrow("scoped");
      expect(() => createKnowledgeRelation(db, { relationId: "relation-missing-party", scope: { kind: "workspace" }, type: "reference", subject: { kind: "page", ref: "page-workspace" }, object: { kind: "party", ref: "party-missing" }, provenance: { kind: "user", ref: "test" }, effectiveFrom: "2026-01-01", actor: "user:test", principal: "user:test" })).toThrow("party");
      expect(() => createKnowledgeRelation(db, { relationId: "relation-missing-company", scope: { kind: "company", companySlug: "alpha-company" }, type: "reference", subject: { kind: "page", ref: "page-alpha" }, object: { kind: "company", ref: "beta-company" }, provenance: { kind: "user", ref: "test" }, effectiveFrom: "2026-01-01", actor: "user:test", principal: "user:test" })).toThrow("company");
      db.close();
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test("never fetches or renders encoded and whitespace-obscured active URLs", () => {
    const fetchBefore = globalThis.fetch;
    let calls = 0;
    globalThis.fetch = (() => { calls += 1; throw new Error("network must not be used"); }) as typeof fetch;
    try {
      for (const value of ["javascript:alert(1)", "java\nscript:alert(1)", "%6a%61vascript%3aalert(1)", " data:text/html,x"]) expect(() => renderKnowledgeMarkdown(value)).toThrow("unsafe external URL");
      expect(renderKnowledgeMarkdown("<script>x</script> https://example.test/a").html).toContain("&lt;script&gt;");
      expect(safeUrl("javascript:alert(1)")).toBe(false);
      expect(calls).toBe(0);
    } finally { globalThis.fetch = fetchBefore; }
  });

  test("keeps page and relation history append-only with optimistic supersession and backlinks", () => {
    const root = mkdtempSync(join(tmpdir(), "rm-knowledge-pages-"));
    try {
      initWorkspace(root); const db = openWorkspaceControlDb(root);
      const first = createKnowledgePage(db, page({ kind: "workspace" }, "page-source", "source"));
      createKnowledgePage(db, page({ kind: "workspace" }, "page-target", "target"));
      const next = supersedeKnowledgePage(db, { ...page({ kind: "workspace" }, "page-source", "source-v2"), expectedVersion: first.version, expectedEventHash: first.eventHash });
      expect(() => supersedeKnowledgePage(db, { ...page({ kind: "workspace" }, "page-source", "source-v3"), expectedVersion: first.version, expectedEventHash: first.eventHash })).toThrow("version");
      expect(listKnowledgePages(db, { scope: { kind: "workspace" } }).map(row => row.slug)).toEqual(["source-v2", "target"]);
      expect(listKnowledgePages(db, { scope: { kind: "workspace" }, asOf: "2025-12-31" })).toHaveLength(0);
      expect(listKnowledgePageHistory(db, { scope: { kind: "workspace" }, pageId: "page-source" })).toHaveLength(2);
      const relation = createKnowledgeRelation(db, { relationId: "relation-reference", scope: { kind: "workspace" }, type: "reference", subject: { kind: "page", ref: "page-source" }, object: { kind: "page", ref: "page-target" }, provenance: { kind: "user", ref: "test" }, effectiveFrom: "2026-01-01", actor: "user:test", principal: "user:test" });
      const changed = supersedeKnowledgeRelation(db, { relationId: relation.relationId, scope: { kind: "workspace" }, type: "reference", subject: { kind: "page", ref: "page-target" }, object: { kind: "page", ref: "page-source" }, provenance: { kind: "user", ref: "test" }, effectiveFrom: "2026-01-01", actor: "user:test", principal: "user:test", expectedVersion: relation.version, expectedEventHash: relation.eventHash });
      expect(changed.version).toBe(2);
      expect(listKnowledgeBacklinks(db, { scope: { kind: "workspace" }, pageId: "page-source" })).toMatchObject([{ direction: "incoming", label: "referenced by" }]);
      expect(listKnowledgeRelationHistory(db, { scope: { kind: "workspace" }, relationId: relation.relationId })).toHaveLength(2);
      expect(() => db.run("UPDATE rm_knowledge_page_events SET title='tamper'")).toThrow("append-only");
      db.close();
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test("reports deterministic source-linked metrics and does not touch protected domains", () => {
    const root = mkdtempSync(join(tmpdir(), "rm-knowledge-pages-"));
    try {
      initWorkspace(root); const db = openWorkspaceControlDb(root);
      const protectedTables = ["rm_ownership_facts", "rm_workspace_user_access_events", "rm_company_membership_events", "rm_company_knowledge_events"];
      const before = Object.fromEntries(protectedTables.map(table => [table, (db.query(`SELECT COUNT(*) AS n FROM ${table}`).get() as { n: number }).n]));
      expect(readKnowledgeMetric(db, { id: "knowledge-page-count", scope: { kind: "workspace" }, limit: 2 })).toMatchObject({ value: null, dataState: "missing", drilldown: [] });
      createKnowledgePage(db, page({ kind: "workspace" }, "page-metric", "metric"));
      const metric = readKnowledgeMetric(db, { id: "knowledge-page-count", scope: { kind: "workspace" }, limit: 1 });
      expect(metric).toMatchObject({ catalogueVersion: 1, value: 1, dataState: "available", drilldown: [{ pageId: "page-metric", provenance: { ref: "https://example.test/source" } }] });
      expect(() => readKnowledgeMetric(db, { id: "knowledge-page-count", scope: { kind: "workspace" }, limit: 0 })).toThrow("limit");
      const after = Object.fromEntries(protectedTables.map(table => [table, (db.query(`SELECT COUNT(*) AS n FROM ${table}`).get() as { n: number }).n]));
      expect(after).toEqual(before);
      db.close();
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
});
