import { createHash, randomUUID } from "node:crypto";
import type { Database } from "bun:sqlite";
import { canonicalJson } from "./canonical-json";

export type KnowledgeScope = { kind: "workspace" } | { kind: "company"; companySlug: string };
export type KnowledgeProvenance = { kind: "user" | "party" | "corporate_record" | "company_knowledge" | "external_snapshot"; ref: string };
export type KnowledgeRelationType = "commercial_counterparty" | "operational_dependency" | "reference";
export type KnowledgeEndpoint = { kind: "page" | "party" | "company"; ref: string };
type StoredScope = { kind: "workspace" | "company"; company: string | null };
type PageEvent = { page_id: string; version: number; event_type: "created" | "superseded"; scope_kind: "workspace" | "company"; company_slug: string | null; slug: string; title: string; body_markdown: string; provenance_kind: KnowledgeProvenance["kind"]; provenance_ref: string; effective_from: string; effective_to_exclusive: string | null; prior_event_hash: string | null; event_hash: string };
type RelationEvent = { relation_id: string; version: number; event_type: "created" | "superseded"; scope_kind: "workspace" | "company"; company_slug: string | null; relation_type: KnowledgeRelationType; subject_kind: KnowledgeEndpoint["kind"]; subject_ref: string; object_kind: KnowledgeEndpoint["kind"]; object_ref: string; provenance_kind: KnowledgeProvenance["kind"]; provenance_ref: string; effective_from: string; effective_to_exclusive: string | null; prior_event_hash: string | null; event_hash: string };

const slug = /^[a-z0-9][a-z0-9-]{0,119}$/;
const identifier = /^[a-z][a-z0-9-]{2,79}$/;
const hash = (value: unknown) => createHash("sha256").update(canonicalJson(value)).digest("hex");
const clean = (value: unknown, name: string, max: number) => { const text = typeof value === "string" ? value.trim() : ""; if (!text || text.length > max) throw new Error(`${name} is required and bounded`); return text; };
const iso = (value: unknown, name: string) => { const text = clean(value, name, 64); const time = new Date(text).valueOf(); if (Number.isNaN(time)) throw new Error(`${name} must be ISO date/time`); return new Date(time).toISOString(); };

function normalisedUrlPayload(value: string): string {
  let result = value.trim().replace(/[\u0000-\u0020\u007f]+/g, "");
  for (let index = 0; index < 3; index += 1) { try { const decoded = decodeURIComponent(result); if (decoded === result) break; result = decoded; } catch { break; } }
  return result.toLowerCase();
}
function unsafeUrlPayload(value: string): boolean { return /^(?:javascript|data):/.test(normalisedUrlPayload(value)); }
export function safeUrl(value: string): boolean { if (unsafeUrlPayload(value)) return false; try { const url = new URL(value); return (url.protocol === "https:" || url.protocol === "http:") && !url.username && !url.password; } catch { return false; } }
const escapeHtml = (value: string) => value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
export function renderKnowledgeMarkdown(markdown: string) {
  if (typeof markdown !== "string" || unsafeUrlPayload(markdown)) throw new Error("unsafe external URL");
  const external = [...markdown.matchAll(/https?:\/\/[^\s)\]]+/gi)].map(match => match[0]);
  if (external.some(url => !safeUrl(url))) throw new Error("unsafe external URL");
  const text = escapeHtml(markdown);
  const html = text.replace(/https?:\/\/[^\s)\]]+/gi, url => `<a href="${escapeHtml(url)}" rel="noopener noreferrer">${escapeHtml(url)}</a>`);
  return { text, html, wikilinks: [...new Set([...markdown.matchAll(/\[\[([a-z0-9][a-z0-9-]{0,119})\]\]/g)].map(match => match[1]!))].sort(), externalLinks: [...new Set(external)].sort() };
}

function scope(input: KnowledgeScope): StoredScope { if (input.kind === "workspace") return { kind: "workspace", company: null }; if (input.kind !== "company" || !slug.test(input.companySlug)) throw new Error("company scope requires a valid slug"); return { kind: "company", company: input.companySlug }; }
function provenance(input: KnowledgeProvenance) { const ref = clean(input.ref, "provenance ref", 512); if (!(["user", "party", "corporate_record", "company_knowledge", "external_snapshot"] as string[]).includes(input.kind)) throw new Error("unsupported provenance kind"); if (input.kind === "external_snapshot" && !safeUrl(ref)) throw new Error("external snapshot provenance must be an http(s) URL"); return { kind: input.kind, ref }; }
function interval(input: { effectiveFrom: string; effectiveToExclusive?: string }) { const from = iso(input.effectiveFrom, "effectiveFrom"); const to = input.effectiveToExclusive ? iso(input.effectiveToExclusive, "effectiveToExclusive") : null; if (to && to <= from) throw new Error("effectiveToExclusive must be after effectiveFrom"); return { from, to }; }
const currentAt = (row: { effective_from: string; effective_to_exclusive: string | null }, asOf?: string) => !asOf || (row.effective_from <= asOf && (!row.effective_to_exclusive || asOf < row.effective_to_exclusive));
function rowPage(row: PageEvent) { return { pageId: row.page_id, version: row.version, eventType: row.event_type, scope: row.scope_kind === "workspace" ? { kind: "workspace" as const } : { kind: "company" as const, companySlug: row.company_slug! }, slug: row.slug, title: row.title, rendered: renderKnowledgeMarkdown(row.body_markdown), provenance: { kind: row.provenance_kind, ref: row.provenance_ref }, effectiveFrom: row.effective_from, effectiveToExclusive: row.effective_to_exclusive, priorEventHash: row.prior_event_hash, eventHash: row.event_hash }; }
function rowRelation(row: RelationEvent) { const labels = relationLabels[row.relation_type]; return { relationId: row.relation_id, version: row.version, eventType: row.event_type, scope: row.scope_kind === "workspace" ? { kind: "workspace" as const } : { kind: "company" as const, companySlug: row.company_slug! }, type: row.relation_type, subject: { kind: row.subject_kind, ref: row.subject_ref }, object: { kind: row.object_kind, ref: row.object_ref }, labels, provenance: { kind: row.provenance_kind, ref: row.provenance_ref }, effectiveFrom: row.effective_from, effectiveToExclusive: row.effective_to_exclusive, priorEventHash: row.prior_event_hash, eventHash: row.event_hash }; }

function assertEndpoint(db: Database, target: StoredScope, value: KnowledgeEndpoint): KnowledgeEndpoint {
  if (!(["page", "party", "company"] as string[]).includes(value.kind) || !identifier.test(value.ref)) throw new Error("relation endpoint is invalid");
  if (value.kind === "page") { const found = db.query("SELECT 1 FROM rm_current_knowledge_pages WHERE page_id=? AND scope_kind=? AND company_slug IS ?").get(value.ref, target.kind, target.company); if (!found) throw new Error("relation page endpoint is missing or outside its scoped knowledge"); }
  if (value.kind === "party" && !db.query("SELECT 1 FROM rm_party_events WHERE party_id=? LIMIT 1").get(value.ref)) throw new Error("relation party endpoint is missing");
  if (value.kind === "company" && (target.kind !== "company" || value.ref !== target.company)) throw new Error("relation company endpoint must be the scoped company");
  return value;
}
export const relationLabels: Record<KnowledgeRelationType, { forward: string; inverse: string }> = { commercial_counterparty: { forward: "buys from", inverse: "sells to" }, operational_dependency: { forward: "depends on", inverse: "supports" }, reference: { forward: "references", inverse: "referenced by" } };

type PageInput = { pageId?: string; scope: KnowledgeScope; slug: string; title: string; bodyMarkdown: string; provenance: KnowledgeProvenance; effectiveFrom: string; effectiveToExclusive?: string; actor: string; principal: string };
function appendPage(db: Database, input: PageInput & { pageId: string; version: number; eventType: "created" | "superseded"; priorEventHash: string | null }) {
  const target = scope(input.scope), pageSlug = clean(input.slug, "slug", 120), pageId = input.pageId;
  if (!identifier.test(pageId) || !slug.test(pageSlug)) throw new Error("invalid page id or slug");
  if (typeof input.bodyMarkdown !== "string" || input.bodyMarkdown.length > 20000) throw new Error("body markdown is bounded");
  const title = clean(input.title, "title", 240), source = provenance(input.provenance), { from, to } = interval(input), actor = clean(input.actor, "actor", 160), principal = clean(input.principal, "principal", 160), createdAt = new Date().toISOString();
  const eventHash = hash({ pageId, version: input.version, eventType: input.eventType, target, pageSlug, title, body: input.bodyMarkdown, source, from, to, priorEventHash: input.priorEventHash, actor, principal, createdAt });
  db.query("INSERT INTO rm_knowledge_page_events(page_id,version,event_type,scope_kind,company_slug,slug,title,body_markdown,provenance_kind,provenance_ref,effective_from,effective_to_exclusive,prior_event_hash,event_hash,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").run(pageId, input.version, input.eventType, target.kind, target.company, pageSlug, title, input.bodyMarkdown, source.kind, source.ref, from, to, input.priorEventHash, eventHash, actor, principal, createdAt);
  return { pageId, version: input.version, eventType: input.eventType, scope: input.scope, slug: pageSlug, title, bodyMarkdown: input.bodyMarkdown, rendered: renderKnowledgeMarkdown(input.bodyMarkdown), provenance: source, effectiveFrom: from, effectiveToExclusive: to, priorEventHash: input.priorEventHash, eventHash };
}
export function createKnowledgePage(db: Database, input: PageInput) { return db.transaction(() => appendPage(db, { ...input, pageId: input.pageId ?? `page-${randomUUID()}`, version: 1, eventType: "created", priorEventHash: null })).immediate(); }
export function supersedeKnowledgePage(db: Database, input: PageInput & { pageId: string; expectedVersion: number; expectedEventHash: string }) { return db.transaction(() => { const prior = db.query("SELECT * FROM rm_current_knowledge_pages WHERE page_id=?").get(input.pageId) as PageEvent | null; if (!prior || prior.scope_kind !== scope(input.scope).kind || prior.company_slug !== scope(input.scope).company) throw new Error("knowledge page is missing in scope"); if (prior.version !== input.expectedVersion || prior.event_hash !== input.expectedEventHash) throw new Error("knowledge page version or hash conflict"); return appendPage(db, { ...input, version: prior.version + 1, eventType: "superseded", priorEventHash: prior.event_hash }); }).immediate(); }
export function listKnowledgePages(db: Database, input: { scope: KnowledgeScope; asOf?: string }) { const target = scope(input.scope), asOf = input.asOf ? iso(input.asOf, "asOf") : undefined; return (db.query("SELECT * FROM rm_current_knowledge_pages WHERE scope_kind=? AND company_slug IS ? ORDER BY slug,page_id").all(target.kind, target.company) as PageEvent[]).filter(row => currentAt(row, asOf)).map(rowPage); }
export function listKnowledgePageHistory(db: Database, input: { scope: KnowledgeScope; pageId: string }) { const target = scope(input.scope); if (!identifier.test(input.pageId)) throw new Error("invalid page id"); return (db.query("SELECT * FROM rm_knowledge_page_events WHERE page_id=? AND scope_kind=? AND company_slug IS ? ORDER BY version").all(input.pageId, target.kind, target.company) as PageEvent[]).map(rowPage); }

type RelationInput = { relationId?: string; scope: KnowledgeScope; type: KnowledgeRelationType; subject: KnowledgeEndpoint; object: KnowledgeEndpoint; provenance: KnowledgeProvenance; effectiveFrom: string; effectiveToExclusive?: string; actor: string; principal: string };
function appendRelation(db: Database, input: RelationInput & { relationId: string; version: number; eventType: "created" | "superseded"; priorEventHash: string | null }) {
  const target = scope(input.scope), relationId = input.relationId, subject = assertEndpoint(db, target, input.subject), object = assertEndpoint(db, target, input.object);
  if (!identifier.test(relationId) || subject.kind === object.kind && subject.ref === object.ref || !relationLabels[input.type]) throw new Error("invalid knowledge relation");
  const source = provenance(input.provenance), { from, to } = interval(input), actor = clean(input.actor, "actor", 160), principal = clean(input.principal, "principal", 160), createdAt = new Date().toISOString();
  const eventHash = hash({ relationId, version: input.version, eventType: input.eventType, target, type: input.type, subject, object, source, from, to, priorEventHash: input.priorEventHash, actor, principal, createdAt });
  db.query("INSERT INTO rm_knowledge_relation_events(relation_id,version,event_type,scope_kind,company_slug,relation_type,subject_kind,subject_ref,object_kind,object_ref,provenance_kind,provenance_ref,effective_from,effective_to_exclusive,prior_event_hash,event_hash,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").run(relationId, input.version, input.eventType, target.kind, target.company, input.type, subject.kind, subject.ref, object.kind, object.ref, source.kind, source.ref, from, to, input.priorEventHash, eventHash, actor, principal, createdAt);
  return { relationId, version: input.version, eventType: input.eventType, scope: input.scope, type: input.type, subject, object, labels: relationLabels[input.type], provenance: source, effectiveFrom: from, effectiveToExclusive: to, priorEventHash: input.priorEventHash, eventHash };
}
export function createKnowledgeRelation(db: Database, input: RelationInput) { return db.transaction(() => appendRelation(db, { ...input, relationId: input.relationId ?? `relation-${randomUUID()}`, version: 1, eventType: "created", priorEventHash: null })).immediate(); }
export function supersedeKnowledgeRelation(db: Database, input: RelationInput & { relationId: string; expectedVersion: number; expectedEventHash: string }) { return db.transaction(() => { const prior = db.query("SELECT * FROM rm_current_knowledge_relations WHERE relation_id=?").get(input.relationId) as RelationEvent | null, target = scope(input.scope); if (!prior || prior.scope_kind !== target.kind || prior.company_slug !== target.company) throw new Error("knowledge relation is missing in scope"); if (prior.version !== input.expectedVersion || prior.event_hash !== input.expectedEventHash) throw new Error("knowledge relation version or hash conflict"); return appendRelation(db, { ...input, version: prior.version + 1, eventType: "superseded", priorEventHash: prior.event_hash }); }).immediate(); }
export function listKnowledgeRelations(db: Database, input: { scope: KnowledgeScope; asOf?: string }) { const target = scope(input.scope), asOf = input.asOf ? iso(input.asOf, "asOf") : undefined; return (db.query("SELECT * FROM rm_current_knowledge_relations WHERE scope_kind=? AND company_slug IS ? ORDER BY relation_id").all(target.kind, target.company) as RelationEvent[]).filter(row => currentAt(row, asOf)).map(rowRelation); }
export function listKnowledgeRelationHistory(db: Database, input: { scope: KnowledgeScope; relationId: string }) { const target = scope(input.scope); if (!identifier.test(input.relationId)) throw new Error("invalid relation id"); return (db.query("SELECT * FROM rm_knowledge_relation_events WHERE relation_id=? AND scope_kind=? AND company_slug IS ? ORDER BY version").all(input.relationId, target.kind, target.company) as RelationEvent[]).map(rowRelation); }
type KnowledgeBacklink = ReturnType<typeof rowRelation> & { direction: "outgoing" | "incoming"; label: string; other: KnowledgeEndpoint };
export function listKnowledgeBacklinks(db: Database, input: { scope: KnowledgeScope; pageId: string; asOf?: string }): KnowledgeBacklink[] { if (!identifier.test(input.pageId)) throw new Error("invalid page id"); return listKnowledgeRelations(db, input).flatMap<KnowledgeBacklink>(relation => relation.subject.kind === "page" && relation.subject.ref === input.pageId ? [{ ...relation, direction: "outgoing", label: relation.labels.forward, other: relation.object }] : relation.object.kind === "page" && relation.object.ref === input.pageId ? [{ ...relation, direction: "incoming", label: relation.labels.inverse, other: relation.subject }] : []); }

export const knowledgeMetricCatalogue = { version: 1, metrics: { "knowledge-page-count": { parameters: ["scope", "asOf?", "limit"], maxLimit: 100 }, "knowledge-relation-count": { parameters: ["scope", "asOf?", "limit"], maxLimit: 100 } } } as const;
type KnowledgeMetricId = keyof typeof knowledgeMetricCatalogue.metrics;
export function readKnowledgeMetric(db: Database, input: { id: KnowledgeMetricId; scope: KnowledgeScope; asOf?: string; limit: number }) {
  if (!knowledgeMetricCatalogue.metrics[input.id] || !Number.isInteger(input.limit) || input.limit < 1 || input.limit > knowledgeMetricCatalogue.metrics[input.id].maxLimit) throw new Error("knowledge metric limit is invalid");
  const pages = listKnowledgePages(db, { scope: input.scope, asOf: input.asOf }), relations = listKnowledgeRelations(db, { scope: input.scope, asOf: input.asOf });
  const drilldown = input.id === "knowledge-page-count" ? pages.slice(0, input.limit).map(page => ({ pageId: page.pageId, slug: page.slug, provenance: page.provenance })) : relations.slice(0, input.limit).map(relation => ({ relationId: relation.relationId, type: relation.type, provenance: relation.provenance }));
  const count = input.id === "knowledge-page-count" ? pages.length : relations.length;
  return { id: input.id, catalogueVersion: knowledgeMetricCatalogue.version, scope: input.scope, asOf: input.asOf ? iso(input.asOf, "asOf") : null, value: count === 0 ? null : count, dataState: count === 0 ? "missing" as const : "available" as const, drilldown };
}
