import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { openWorkspaceControlReadOnlyDb } from "../../core/workspace-control";
import { getWorkspaceUserAccess } from "../../core/workspace-access";
import { listKnowledgePages, type KnowledgeScope } from "../../core/knowledge-pages";
import { okResponse } from "./_shared";

/** URL scope is authorized by the catalog before any knowledge content opens. */
export function readTaskKnowledgePage(config: ServerConfig, scope: KnowledgeScope, reference: string): Response {
  if (!/^[a-z0-9][a-z0-9-]{0,119}$/.test(reference)) throw ApiError.notFound("Videnssiden findes ikke eller er ikke tilgængelig.");
  const db = openWorkspaceControlReadOnlyDb(config.workspaceRoot);
  try {
    const principal = config.requestPrincipal;
    if (scope.kind === "workspace" && principal?.via !== "localhost-trusted" && principal?.via !== "shared-secret"
      && (!principal?.userId || getWorkspaceUserAccess(db, principal.userId).workspaceRole !== "workspace_owner")) throw ApiError.unauthorized("missing or invalid credentials");
    const page = listKnowledgePages(db, { scope, asOf: new Date().toISOString() }).find(page => page.pageId === reference || page.slug === reference);
    if (!page) throw ApiError.notFound("Videnssiden findes ikke eller er ikke tilgængelig.");
    const row = db.query("SELECT body_markdown FROM rm_current_knowledge_pages WHERE page_id=? AND scope_kind=? AND company_slug IS ?").get(page.pageId, scope.kind, scope.kind === "company" ? scope.companySlug : null) as { body_markdown: string };
    return okResponse({ page: { ...page, bodyMarkdown: row.body_markdown } });
  } finally { db.close(); }
}
