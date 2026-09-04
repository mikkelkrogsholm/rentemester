import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { createKnowledgePage, listKnowledgePages, renderKnowledgeMarkdown, safeUrl } from "../../src/core/knowledge-pages";

describe("knowledge pages",()=>{
  test("keeps source-bound escaped pages append-only without network rendering",()=>{const root=mkdtempSync(join(tmpdir(),"rm-knowledge-pages-"));try{initWorkspace(root);const db=openWorkspaceControlDb(root);const page=createKnowledgePage(db,{pageId:"page-synthetic",scope:{kind:"workspace"},slug:"synthetic-page",title:"Synthetic",bodyMarkdown:"<script>x</script> [[related-page]] https://example.test/reference",provenance:{kind:"external_snapshot",ref:"https://example.test/source"},effectiveFrom:"2026-01-01",actor:"user:test",principal:"user:test"});expect(page.rendered).toMatchObject({wikilinks:["related-page"],externalLinks:["https://example.test/reference"]});expect(page.rendered.text).toContain("&lt;script&gt;");const pages=listKnowledgePages(db,{scope:{kind:"workspace"}});expect(pages).toHaveLength(1);expect(()=>db.run("UPDATE rm_knowledge_page_events SET title='tamper'")).toThrow("append-only");expect(safeUrl("javascript:alert(1)")).toBe(false);expect(()=>renderKnowledgeMarkdown("javascript:alert(1)")).toThrow("unsafe external URL");db.close();}finally{rmSync(root,{recursive:true,force:true});}});
});
