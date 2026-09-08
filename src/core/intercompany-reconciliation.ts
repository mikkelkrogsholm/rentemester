/**
 * Workspace-owned intercompany mappings and read-only reciprocal balances.
 * This module never migrates or writes a legal-entity ledger. Different
 * currencies are deliberately not compared and no tolerance is inferred.
 */
import { type Database, type SQLQueryBindings } from "bun:sqlite";
import { createHash } from "node:crypto";
import type { ResolveActorInput } from "./actor";
import { resolveActor } from "./actor";
import { assertSchemaCompatibility } from "./schema-version";
import { verifyAuditChain } from "./ledger";
import { fromOre, toOre } from "./money";
import { companyPaths } from "./paths";
import { companyRootForSlug, listWorkspaceCompanies } from "./workspace";
import { insertWorkspaceAudit } from "./workspace-control";
import { openCurrentLedgerReadOnly } from "./ledger-inspection";
import { parseGroupAsOf, readCurrentGroupManifest, type EffectiveInterval, type GroupManifest } from "./group-manifest";

const IDENTIFIER = /^[a-z][a-z0-9-]{0,63}$/;
const ACCOUNT = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;
const MAX_ACCOUNTS = 128;
const MAX_EVIDENCE_REFS = 32;

export type IntercompanyPosition = "receivable" | "payable";
export type IntercompanyMapping = EffectiveInterval & {
  id: string;
  groupId: string;
  leftCompanySlug: string;
  rightCompanySlug: string;
  leftAccountNos: string[];
  rightAccountNos: string[];
  leftPosition: IntercompanyPosition;
  rightPosition: IntercompanyPosition;
  evidenceRefs: string[];
};

type MappingEvent = {
  id: number;
  mapping_id: string;
  event_type: "proposed" | "approved" | "revoked";
  mapping_hash: string;
  canonical_mapping: string;
  previous_hash: string | null;
  event_hash: string;
  actor: string;
  created_at: string;
};

function compare(a: string, b: string): number { return a < b ? -1 : a > b ? 1 : 0; }
function activeAt(interval: EffectiveInterval, asOf: string): boolean {
  return interval.validFrom <= asOf && (interval.validToExclusive == null || asOf < interval.validToExclusive);
}
function overlaps(a: EffectiveInterval, b: EffectiveInterval): boolean {
  return (a.validToExclusive == null || b.validFrom < a.validToExclusive) &&
    (b.validToExclusive == null || a.validFrom < b.validToExclusive);
}
function covers(container: EffectiveInterval, target: EffectiveInterval): boolean {
  return container.validFrom <= target.validFrom &&
    (container.validToExclusive == null || (target.validToExclusive != null && container.validToExclusive >= target.validToExclusive));
}
function requiredString(value: unknown, label: string): string {
  if (typeof value !== "string" || !value.trim()) throw new Error(`${label} is required`);
  return value.trim().normalize("NFC");
}
function identifier(value: unknown, label: string): string {
  const result = requiredString(value, label);
  if (!IDENTIFIER.test(result)) throw new Error(`${label} must be a lowercase stable identifier`);
  return result;
}
function date(value: unknown, label: string): string {
  try { return parseGroupAsOf(value); }
  catch { throw new Error(`${label} must be a real ISO date`); }
}
function stringList(value: unknown, label: string, max: number, pattern?: RegExp): string[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > max) throw new Error(`${label} must contain 1 through ${max} values`);
  const rows = value.map((entry, index) => {
    const result = requiredString(entry, `${label}[${index}]`);
    if (result.length > 256 || (pattern && !pattern.test(result))) throw new Error(`${label}[${index}] is invalid`);
    return result;
  });
  if (new Set(rows).size !== rows.length) throw new Error(`${label} must be unique`);
  return rows.sort(compare);
}

/** Validate a mapping only against explicit structure and portable fields. */
export function parseIntercompanyMapping(input: unknown, groupManifest: GroupManifest): IntercompanyMapping {
  if (!input || typeof input !== "object" || Array.isArray(input)) throw new Error("intercompany mapping must be an object");
  const row = input as Record<string, unknown>;
  const id = identifier(row.id, "mapping.id");
  const groupId = identifier(row.groupId, "mapping.groupId");
  const group = groupManifest.groups.find((candidate) => candidate.id === groupId);
  if (!group) throw new Error("intercompany mapping references an unknown group");
  const leftCompanySlug = requiredString(row.leftCompanySlug, "mapping.leftCompanySlug");
  const rightCompanySlug = requiredString(row.rightCompanySlug, "mapping.rightCompanySlug");
  if (leftCompanySlug === rightCompanySlug) throw new Error("intercompany mapping must bind two distinct companies");
  const validFrom = date(row.validFrom, "mapping.validFrom");
  const validToExclusive = row.validToExclusive == null ? undefined : date(row.validToExclusive, "mapping.validToExclusive");
  if (validToExclusive && validToExclusive <= validFrom) throw new Error("intercompany mapping must have a non-empty half-open interval");
  const effective = { validFrom, ...(validToExclusive ? { validToExclusive } : {}) };
  for (const slug of [leftCompanySlug, rightCompanySlug]) {
    if (!group.memberships.some((membership) => membership.companySlug === slug && covers(membership, effective))) {
      throw new Error("intercompany mapping must be fully covered by active group memberships");
    }
  }
  const leftPosition = requiredString(row.leftPosition, "mapping.leftPosition") as IntercompanyPosition;
  const rightPosition = requiredString(row.rightPosition, "mapping.rightPosition") as IntercompanyPosition;
  if (!(["receivable", "payable"] as const).includes(leftPosition) || !(["receivable", "payable"] as const).includes(rightPosition) || leftPosition === rightPosition) {
    throw new Error("intercompany mapping positions must be complementary receivable and payable");
  }
  return {
    id,
    groupId,
    leftCompanySlug,
    rightCompanySlug,
    leftAccountNos: stringList(row.leftAccountNos, "mapping.leftAccountNos", MAX_ACCOUNTS, ACCOUNT),
    rightAccountNos: stringList(row.rightAccountNos, "mapping.rightAccountNos", MAX_ACCOUNTS, ACCOUNT),
    leftPosition,
    rightPosition,
    evidenceRefs: stringList(row.evidenceRefs, "mapping.evidenceRefs", MAX_EVIDENCE_REFS),
    ...effective,
  };
}

export function canonicalizeIntercompanyMapping(mapping: IntercompanyMapping): string {
  return JSON.stringify({
    id: mapping.id,
    groupId: mapping.groupId,
    leftCompanySlug: mapping.leftCompanySlug,
    rightCompanySlug: mapping.rightCompanySlug,
    leftAccountNos: [...mapping.leftAccountNos].sort(compare),
    rightAccountNos: [...mapping.rightAccountNos].sort(compare),
    leftPosition: mapping.leftPosition,
    rightPosition: mapping.rightPosition,
    evidenceRefs: [...mapping.evidenceRefs].sort(compare),
    validFrom: mapping.validFrom,
    ...(mapping.validToExclusive ? { validToExclusive: mapping.validToExclusive } : {}),
  });
}

function sha(value: string): string { return createHash("sha256").update(value).digest("hex"); }
function eventHash(previous: string | null, event: Omit<MappingEvent, "event_hash">): string {
  return sha(JSON.stringify({ previousHash: previous, id: event.id, mappingId: event.mapping_id, eventType: event.event_type, mappingHash: event.mapping_hash, canonicalMapping: event.canonical_mapping, actor: event.actor, createdAt: event.created_at }));
}
function readEvents(db: Database): MappingEvent[] {
  const rows = db.query("SELECT id,mapping_id,event_type,mapping_hash,canonical_mapping,previous_hash,event_hash,actor,created_at FROM rm_intercompany_mapping_events ORDER BY id").all() as MappingEvent[];
  let previous: string | null = null;
  for (const row of rows) {
    if (row.previous_hash !== previous || row.event_hash !== eventHash(previous, row)) throw new Error("intercompany mapping event hash-chain is invalid");
    previous = row.event_hash;
  }
  return rows;
}
function currentEvents(events: readonly MappingEvent[]): Map<string, MappingEvent> {
  const current = new Map<string, MappingEvent>();
  for (const event of events) current.set(event.mapping_id, event);
  return current;
}
function decoded(event: MappingEvent, groupManifest: GroupManifest): IntercompanyMapping {
  const mapping = parseIntercompanyMapping(JSON.parse(event.canonical_mapping), groupManifest);
  if (mapping.id !== event.mapping_id || sha(event.canonical_mapping) !== event.mapping_hash) throw new Error("intercompany mapping evidence is invalid");
  return mapping;
}

export function readIntercompanyMappingState(db: Database, workspaceRoot: string, mappingId: string): { mapping: IntercompanyMapping; mappingHash: string; status: MappingEvent["event_type"]; actor: string } | null {
  const group = readCurrentGroupManifest(db, workspaceRoot);
  if (!group) return null;
  const event = currentEvents(readEvents(db)).get(mappingId) ?? null;
  return event ? { mapping: decoded(event, group.manifest), mappingHash: event.mapping_hash, status: event.event_type, actor: event.actor } : null;
}
function assertNoOverlap(mapping: IntercompanyMapping, approved: readonly IntercompanyMapping[]): void {
  for (const other of approved) {
    if (!overlaps(mapping, other)) continue;
    const selected = (company: string, accounts: readonly string[], candidate: IntercompanyMapping): readonly string[] | null =>
      candidate.leftCompanySlug === company ? candidate.leftAccountNos : candidate.rightCompanySlug === company ? candidate.rightAccountNos : null;
    for (const [company, accounts] of [[mapping.leftCompanySlug, mapping.leftAccountNos], [mapping.rightCompanySlug, mapping.rightAccountNos]] as const) {
      const otherAccounts = selected(company, accounts, other);
      if (otherAccounts && accounts.some((account) => otherAccounts.includes(account))) throw new Error("an account may not appear in overlapping approved intercompany mappings");
    }
  }
}
function appendEvent(db: Database, eventType: MappingEvent["event_type"], mapping: IntercompanyMapping, audit: ResolveActorInput): MappingEvent {
  const rows = readEvents(db);
  const previous = rows.at(-1)?.event_hash ?? null;
  const canonical = canonicalizeIntercompanyMapping(mapping);
  const actor = resolveActor(audit).auditActor;
  const event = { id: (rows.at(-1)?.id ?? 0) + 1, mapping_id: mapping.id, event_type: eventType, mapping_hash: sha(canonical), canonical_mapping: canonical, previous_hash: previous, actor, created_at: new Date().toISOString() };
  const complete: MappingEvent = { ...event, event_hash: eventHash(previous, event) };
  db.query("INSERT INTO rm_intercompany_mapping_events (id,mapping_id,event_type,mapping_hash,canonical_mapping,previous_hash,event_hash,actor,created_at) VALUES (?,?,?,?,?,?,?,?,?)").run(complete.id, complete.mapping_id, complete.event_type, complete.mapping_hash, complete.canonical_mapping, complete.previous_hash, complete.event_hash, complete.actor, complete.created_at);
  insertWorkspaceAudit(db, { ...audit, eventType: `intercompany_mapping_${eventType}`, entityType: "intercompany_mapping", entityId: mapping.id });
  return complete;
}

/** Proposals are inert until a distinct actor approves them. */
export function proposeIntercompanyMapping(db: Database, workspaceRoot: string, input: unknown, audit: ResolveActorInput): { mappingId: string; mappingHash: string; status: "proposed" } {
  return db.transaction(() => {
    const group = readCurrentGroupManifest(db, workspaceRoot);
    if (!group) throw new Error("group structure must be configured before intercompany mappings");
    const mapping = parseIntercompanyMapping(input, group.manifest);
    const state = currentEvents(readEvents(db)).get(mapping.id);
    if (state && state.event_type !== "revoked") throw new Error("intercompany mapping id already has an active lifecycle");
    const event = appendEvent(db, "proposed", mapping, audit);
    return { mappingId: mapping.id, mappingHash: event.mapping_hash, status: "proposed" as const };
  }).immediate();
}

/** Approval is four-eyes: the proposal actor may never approve their own mapping. */
export function approveIntercompanyMapping(db: Database, workspaceRoot: string, mappingId: string, mappingHash: string, audit: ResolveActorInput): { mappingId: string; mappingHash: string; status: "approved" } {
  return db.transaction(() => {
    const group = readCurrentGroupManifest(db, workspaceRoot);
    if (!group) throw new Error("group structure is unavailable");
    const events = readEvents(db);
    const proposal = currentEvents(events).get(mappingId);
    if (!proposal || proposal.event_type !== "proposed" || proposal.mapping_hash !== mappingHash) throw new Error("exact pending intercompany mapping proposal was not found");
    const actor = resolveActor(audit).auditActor;
    if (actor === proposal.actor) throw new Error("intercompany mapping approval requires a distinct reviewer");
    const mapping = decoded(proposal, group.manifest);
    const approved = [...currentEvents(events).values()].filter((event) => event.event_type === "approved").map((event) => decoded(event, group.manifest));
    assertNoOverlap(mapping, approved);
    const event = appendEvent(db, "approved", mapping, audit);
    return { mappingId, mappingHash: event.mapping_hash, status: "approved" as const };
  }).immediate();
}

export function revokeIntercompanyMapping(db: Database, workspaceRoot: string, mappingId: string, audit: ResolveActorInput): { mappingId: string; status: "revoked" } {
  return db.transaction(() => {
    const group = readCurrentGroupManifest(db, workspaceRoot);
    if (!group) throw new Error("group structure is unavailable");
    const current = currentEvents(readEvents(db)).get(mappingId);
    if (!current || current.event_type !== "approved") throw new Error("approved intercompany mapping was not found");
    appendEvent(db, "revoked", decoded(current, group.manifest), audit);
    return { mappingId, status: "revoked" as const };
  }).immediate();
}

type SourceRef = { entryId: number; entryNo: string; lineId: number; accountNo: string; transactionDate: string; amount: number };
type Side = { companySlug: string; currency: string; position: IntercompanyPosition; balance: number; accountNos: string[]; sourceRefs: SourceRef[]; sourceSnapshot: { ledgerHeadHash: string | null; entryCount: number; selectionHash: string } };
export type IntercompanyReconciliationRow =
  | { mappingId?: string; status: "not-comparable"; reason: "blocked"; blockers: string[] }
  | { mappingId: string; mappingHash: string; left: Side; right: Side; status: "not-comparable"; reason: "currency-mismatch"; blockers: string[] }
  | { mappingId: string; mappingHash: string; left: Side; right: Side; status: "matched" | "mismatch"; difference: number; reason: "exact-native-currency-difference"; blockers: string[] };

function readSide(workspaceRoot: string, companySlug: string, accountNos: readonly string[], position: IntercompanyPosition, asOf: string): Side {
  const root = companyRootForSlug(workspaceRoot, companySlug);
  const db = openCurrentLedgerReadOnly(companyPaths(root).db);
  try {
    db.exec("PRAGMA query_only = ON; PRAGMA foreign_keys = ON");
    assertSchemaCompatibility(db);
    const audit = verifyAuditChain(db, { companyRoot: root });
    if (!audit.ok) throw new Error("ledger audit verification failed");
    const accounts = db.query(`SELECT account_no,active FROM accounts WHERE account_no IN (${accountNos.map(() => "?").join(",")})`).all(...accountNos) as Array<{ account_no: string; active: number }>;
    if (accounts.length !== accountNos.length || accounts.some((account) => account.active !== 1)) throw new Error("mapped intercompany account is missing or inactive");
    const bindings: SQLQueryBindings[] = [...accountNos, asOf];
    const lines = db.query(`SELECT jl.id AS line_id,a.account_no,jl.debit_amount,jl.credit_amount,je.id AS entry_id,je.entry_no,je.transaction_date FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id JOIN journal_entries je ON je.id=jl.journal_entry_id WHERE a.account_no IN (${accountNos.map(() => "?").join(",")}) AND je.transaction_date<=? ORDER BY je.id,jl.id`).all(...bindings) as Array<{ line_id: number; account_no: string; debit_amount: number; credit_amount: number; entry_id: number; entry_no: string; transaction_date: string }>;
    const signedOre = lines.reduce((sum, line) => sum + (position === "receivable" ? toOre(line.debit_amount) - toOre(line.credit_amount) : toOre(line.credit_amount) - toOre(line.debit_amount)), 0n);
    const company = db.query("SELECT currency FROM companies WHERE id=1").get() as { currency: string } | null;
    if (!company?.currency) throw new Error("company functional currency is unavailable");
    const head = db.query("SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1").get() as { entry_hash: string } | null;
    const sourceRefs = lines.map((line) => ({ entryId: line.entry_id, entryNo: line.entry_no, lineId: line.line_id, accountNo: line.account_no, transactionDate: line.transaction_date, amount: fromOre(position === "receivable" ? toOre(line.debit_amount) - toOre(line.credit_amount) : toOre(line.credit_amount) - toOre(line.debit_amount)) }));
    const selectionHash = sha(JSON.stringify({ companySlug, asOf, accountNos: [...accountNos], position, sourceRefs }));
    return { companySlug, currency: company.currency.trim().toUpperCase(), position, balance: fromOre(signedOre), accountNos: [...accountNos], sourceRefs, sourceSnapshot: { ledgerHeadHash: head?.entry_hash ?? null, entryCount: audit.entries, selectionHash } };
  } finally { db.close(); }
}

/** Exact, same-currency, read-only reconciliation over approved mappings. */
export function buildIntercompanyReconciliation(db: Database, workspaceRoot: string, visibleCompanySlugs: ReadonlySet<string>, asOfInput: string): { scope: "intercompany-reconciliation"; asOf: string; rows: IntercompanyReconciliationRow[] } {
  const asOf = parseGroupAsOf(asOfInput);
  const group = readCurrentGroupManifest(db, workspaceRoot);
  if (!group) return { scope: "intercompany-reconciliation", asOf, rows: [] };
  const companies = new Map(listWorkspaceCompanies(workspaceRoot).map((company) => [company.slug, company]));
  const approved = [...currentEvents(readEvents(db)).values()].filter((event) => event.event_type === "approved").map((event) => ({ event, mapping: decoded(event, group.manifest) })).filter(({ mapping }) => activeAt(mapping, asOf));
  const rows = approved.map(({ event, mapping }): IntercompanyReconciliationRow => {
    if (!visibleCompanySlugs.has(mapping.leftCompanySlug) || !visibleCompanySlugs.has(mapping.rightCompanySlug)) return { status: "not-comparable", reason: "blocked", blockers: ["both mapped companies must be visible"] };
    if (companies.get(mapping.leftCompanySlug)?.archived || companies.get(mapping.rightCompanySlug)?.archived) return { mappingId: mapping.id, status: "not-comparable", reason: "blocked", blockers: ["mapped company is archived"] };
    try {
      const left = readSide(workspaceRoot, mapping.leftCompanySlug, mapping.leftAccountNos, mapping.leftPosition, asOf);
      const right = readSide(workspaceRoot, mapping.rightCompanySlug, mapping.rightAccountNos, mapping.rightPosition, asOf);
      if (left.currency !== right.currency) return { mappingId: mapping.id, mappingHash: event.mapping_hash, left, right, status: "not-comparable", reason: "currency-mismatch", blockers: ["no approved FX policy exists"] };
      const differenceOre = toOre(left.balance) - toOre(right.balance);
      return { mappingId: mapping.id, mappingHash: event.mapping_hash, left, right, status: differenceOre === 0n ? "matched" : "mismatch", difference: fromOre(differenceOre), reason: "exact-native-currency-difference", blockers: [] };
    } catch {
      return { mappingId: mapping.id, status: "not-comparable", reason: "blocked", blockers: ["source ledger is unavailable or failed integrity validation"] };
    }
  });
  return { scope: "intercompany-reconciliation", asOf, rows };
}
