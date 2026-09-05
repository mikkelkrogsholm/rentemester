import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { createHash, randomBytes } from "node:crypto";
import { companyPaths } from "./paths";
import { migrate, openDb } from "./db";
import {
  createSystemBackup,
  packBackupArchive,
  type ManifestFile,
} from "./system-backups";
import { restoreSystemBackup } from "./system-restore";
import {
  companyRootForSlug,
  initWorkspace,
  listWorkspaceCompanies,
  loadWorkspaceManifest,
  saveWorkspaceManifest,
  type WorkspaceManifest,
} from "./workspace";
import { listWorkspaceMembers, type CompanyRole, type WorkspaceRole } from "./workspace-access";
import { openWorkspaceControlDb, workspaceControlPaths } from "./workspace-control";
import { createTar, dirToTarEntries, extractTar, readTar } from "./tar";
import { getReleaseProvenance, isReleaseProvenance, type ReleaseProvenance } from "./release-provenance";
import { promoteTempFileExclusive, writeFileAtomic, writeTempFileFor } from "./atomic-file";
import { removePathWithRetry, renamePathWithRetry } from "./fs-cleanup";

const SNAPSHOT_RULE_ID = "RENTEMESTER-WORKSPACE-SNAPSHOT-001";
const SAFE_PORTABLE_CONFIG = new Set(["backup-lock.json", "backup-manifest.pub", "policy.yaml"]);

type SnapshotAccessPlan = {
  version: 1;
  recovery: "bootstrap-owner-then-reinvite";
  users: Array<{
    name: string;
    email: string;
    workspaceRole: WorkspaceRole;
    memberships: Array<{ companySlug: string; role: CompanyRole }>;
  }>;
};

export type WorkspaceSnapshotManifestV1 = {
  version: 1;
  createdAt: string;
  provenance: ReleaseProvenance;
  credentialPolicy: "omit-all-auth-and-operational-credentials-v1";
  workspaceManifest: ManifestFile;
  accessPlan: ManifestFile;
  /** Non-credential operating knowledge is portable; auth state remains omitted. */
  companyKnowledge?: ManifestFile;
  /** Source-backed legal ownership history is portable; credentials remain omitted. */
  ownershipGraph?: ManifestFile;
  /** Canonical party provenance and immutable corporate evidence; no auth data. */
  workspaceRegistry?: ManifestFile;
  /** Append-only, workspace-only intercompany evidence lifecycle; no credentials. */
  intercompanyDispositions?: ManifestFile;
  workspaceInbox?: ManifestFile;
  companies: Array<{
    slug: string;
    name: string;
    archived: boolean;
    backup: ManifestFile;
  }>;
};

export type CreateWorkspaceSnapshotResult = {
  ok: boolean;
  snapshotPath?: string;
  sha256?: string;
  sha256Path?: string;
  companyCount?: number;
  accessIdentityCount?: number;
  appliedRules: string[];
  errors: string[];
};

export type RestoreWorkspaceSnapshotResult = {
  ok: boolean;
  targetWorkspaceRoot?: string;
  companyCount?: number;
  accessRecoveryPlanPath?: string;
  nextStep?: "bootstrap-owner-then-reinvite";
  appliedRules: string[];
  errors: string[];
};

function sha256(content: Uint8Array): string {
  return createHash("sha256").update(content).digest("hex");
}

function fileEvidence(root: string, path: string): ManifestFile {
  const content = readFileSync(path);
  return {
    path: path.slice(root.length + 1).replaceAll("\\", "/"),
    sha256: sha256(content),
    sizeBytes: content.byteLength,
  };
}

function validInstant(value?: string): string | null {
  const instant = value ?? new Date().toISOString();
  return Number.isNaN(Date.parse(instant)) ? null : new Date(instant).toISOString();
}

function snapshotError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return message.replace(/[\r\n]+/g, " ").slice(0, 500);
}

export function createWorkspaceSnapshot(
  workspaceRoot: string,
  input: { outPath: string; createdAt?: string; createdBy?: string; createdByProgram?: string },
): CreateWorkspaceSnapshotResult {
  const createdAt = validInstant(input.createdAt);
  const outPath = input.outPath?.trim();
  if (!createdAt || !outPath) {
    return { ok: false, appliedRules: [SNAPSHOT_RULE_ID], errors: ["outPath and a valid createdAt are required"] };
  }
  if (existsSync(outPath)) {
    return { ok: false, appliedRules: [SNAPSHOT_RULE_ID], errors: ["snapshot destination already exists"] };
  }
  const companies = listWorkspaceCompanies(workspaceRoot);
  if (companies.length === 0) {
    return { ok: false, appliedRules: [SNAPSHOT_RULE_ID], errors: ["workspace has no registered companies"] };
  }
  const staging = mkdtempSync(join(tmpdir(), "rentemester-workspace-snapshot-"));
  try {
    const workspaceManifestPath = join(staging, "workspace.json");
    saveWorkspaceManifest(staging, loadWorkspaceManifest(workspaceRoot));
    const controlDbPath = workspaceControlPaths(workspaceRoot).db;
    const accessPlan: SnapshotAccessPlan = { version: 1, recovery: "bootstrap-owner-then-reinvite", users: [] };
    if (existsSync(controlDbPath)) {
      const controlDb = openWorkspaceControlDb(workspaceRoot);
      try {
        accessPlan.users = listWorkspaceMembers(controlDb, workspaceRoot).map((member) => ({
          name: member.name,
          email: member.email,
          workspaceRole: member.workspaceRole,
          memberships: member.memberships.map(({ companySlug, role }) => ({ companySlug, role })),
        }));
      } finally { controlDb.close(); }
    }
    const accessPlanPath = join(staging, "access-plan.json");
    writeFileAtomic(accessPlanPath, `${JSON.stringify(accessPlan, null, 2)}\n`);
    let companyKnowledge: ManifestFile | undefined;
    let ownershipGraph: ManifestFile | undefined;
    let workspaceRegistry: ManifestFile | undefined;
    let intercompanyDispositions: ManifestFile | undefined;
    let workspaceInbox: ManifestFile | undefined;
    if (existsSync(controlDbPath)) {
      const controlDb = openWorkspaceControlDb(workspaceRoot);
      try {
        const assertions = controlDb.query("SELECT assertion_id,company_slug,predicate,value_json,value_hash,source_kind,source_ref,valid_from,valid_to_exclusive,certainty,actor,principal_kind,principal_id,created_at FROM rm_company_knowledge_assertions ORDER BY id").all();
        const events = controlDb.query("SELECT assertion_id,event_type,reason,supersedes_assertion_id,actor,principal_kind,principal_id,created_at FROM rm_company_knowledge_events ORDER BY id").all();
        if (assertions.length > 0 || events.length > 0) {
          const path = join(staging, "company-knowledge.json");
          writeFileAtomic(path, `${JSON.stringify({ version: 1, assertions, events })}\n`);
          companyKnowledge = fileEvidence(staging, path);
        }
      } finally { controlDb.close(); }
    }
    if (existsSync(controlDbPath)) {
      const controlDb = openWorkspaceControlDb(workspaceRoot);
      try {
        const dispositions = controlDb.query("SELECT disposition_id,canonical_payload,payload_hash,created_at FROM rm_intercompany_dispositions ORDER BY disposition_id").all();
        const events = controlDb.query("SELECT disposition_id,event_type,payload_hash,canonical_payload,actor,principal_kind,principal_id,previous_hash,event_hash,created_at FROM rm_intercompany_disposition_events ORDER BY id").all();
        const links = controlDb.query("SELECT disposition_id,side,company_slug,journal_entry_id,journal_entry_no,journal_entry_hash,ledger_head_hash,linked_at,actor,principal_kind,principal_id FROM rm_intercompany_disposition_journal_links ORDER BY disposition_id,side").all();
        const lifecycleEvents = controlDb.query("SELECT disposition_id,event_type,payload_hash,canonical_payload,actor,principal_kind,principal_id,previous_hash,event_hash,created_at FROM rm_intercompany_disposition_lifecycle_events ORDER BY id").all();
        if ([dispositions, events, links, lifecycleEvents].some((rows) => rows.length > 0)) {
          const path = join(staging, "intercompany-dispositions.json");
          writeFileAtomic(path, `${JSON.stringify({ version: 1, dispositions, events, links, lifecycleEvents })}\n`);
          intercompanyDispositions = fileEvidence(staging, path);
        }
      } finally { controlDb.close(); }
    }
    if (existsSync(controlDbPath)) { const controlDb=openWorkspaceControlDb(workspaceRoot);try{const sources=(controlDb.query("SELECT source_id,visibility_anchor_slug,idempotency_key,original_bytes,sha256,filename,mime_type,transport,transport_identity,received_at,metadata_json,created_by,created_at FROM rm_workspace_inbox_sources ORDER BY source_id").all() as any[]).map(row=>({...row,original_bytes_base64:Buffer.from(row.original_bytes).toString("base64"),original_bytes:undefined})),events=controlDb.query("SELECT source_id,event_type,payload_hash,canonical_payload,actor,created_at FROM rm_workspace_inbox_events ORDER BY id").all(),assignments=controlDb.query("SELECT source_id,company_slug,state,document_id,document_no,assigned_by,assigned_at,completed_at FROM rm_workspace_inbox_assignments ORDER BY source_id,company_slug").all(),exceptions=controlDb.query("SELECT source_id,code,required_action,opened_at,resolved_at FROM rm_workspace_inbox_exceptions ORDER BY source_id").all(),claims=controlDb.query("SELECT source_id,company_slug,source_hash,state,claim_id,lease_expires_at,document_id,document_no,created_at,updated_at FROM rm_workspace_inbox_handoff_claims ORDER BY source_id,company_slug").all();if([sources,events,assignments,exceptions,claims].some(x=>x.length)){const path=join(staging,"workspace-inbox.json");writeFileAtomic(path,`${JSON.stringify({version:1,sources,events,assignments,exceptions,claims})}\n`);workspaceInbox=fileEvidence(staging,path);}}finally{controlDb.close();}}
    if (existsSync(controlDbPath)) {
      const controlDb = openWorkspaceControlDb(workspaceRoot);
      try {
        const parties = controlDb.query("SELECT party_id,event_type,canonical_kind,payload_hash,canonical_payload,actor,created_at FROM rm_party_events ORDER BY id").all();
        const aliases = controlDb.query("SELECT party_id,alias,source,observed_at,review_state,payload_hash,actor,created_at FROM rm_party_alias_assertions ORDER BY id").all();
        const assertions = controlDb.query("SELECT party_id,field,value,source,observed_at,review_state,payload_hash,actor,created_at FROM rm_party_field_assertions ORDER BY id").all();
        const roles = controlDb.query("SELECT party_id,company_slug,role,defaults_json FROM rm_party_company_roles ORDER BY party_id,company_slug,role").all();
        const legacyLinks = controlDb.query("SELECT company_slug,legacy_kind,legacy_id,party_id,actor,created_at FROM rm_party_legacy_links ORDER BY company_slug,legacy_kind,legacy_id").all();
        const legacyMappings = controlDb.query("SELECT company_slug,legacy_kind,legacy_id,party_id,party_role,event_type,version,prior_event_hash,event_hash,contact_snapshot,contact_fingerprint,evidence_json,plan_hash,idempotency_key_hash,idempotency_payload_hash,reason,actor,principal,created_at FROM rm_legacy_party_mapping_events ORDER BY id").all();
        const identifiers = controlDb.query("SELECT party_id,jurisdiction,identifier_kind,identifier FROM rm_party_identifiers ORDER BY party_id,jurisdiction,identifier_kind,identifier").all();
        const bytes = (controlDb.query("SELECT record_id,original_bytes,sha256,filename,source,received_at,record_date,retention_until,uploader,sensitivity FROM rm_corporate_record_bytes ORDER BY record_id").all() as any[]).map(row => ({...row, original_bytes_base64: Buffer.from(row.original_bytes).toString("base64"), original_bytes: undefined}));
        const events = controlDb.query("SELECT record_id,event_type,record_type,payload_hash,canonical_payload,actor,created_at FROM rm_corporate_record_events ORDER BY id").all();
        const links = controlDb.query("SELECT record_id,link_type,link_id FROM rm_corporate_record_links ORDER BY record_id,link_type,link_id").all();
        const scopes = controlDb.query("SELECT record_id,scope_kind,scope_id,actor,created_at FROM rm_corporate_record_scope_assertions ORDER BY id").all();
        if ([parties,aliases,assertions,roles,legacyLinks,legacyMappings,identifiers,bytes,events,links,scopes].some(rows => rows.length)) {
          const path = join(staging, "workspace-registry.json");
          writeFileAtomic(path, `${JSON.stringify({version:2,parties,aliases,assertions,roles,legacyLinks,legacyMappings,identifiers,corporate:{bytes,events,links,scopes}})}\n`);
          workspaceRegistry = fileEvidence(staging, path);
        }
      } finally { controlDb.close(); }
    }
    if (existsSync(controlDbPath)) {
      const controlDb = openWorkspaceControlDb(workspaceRoot);
      try {
        const snapshots = controlDb.query("SELECT snapshot_id,source,observed_at,snapshot_hash,diff_hash,canonical_facts,diff_json,actor,principal_kind,principal_id,created_at FROM rm_ownership_source_snapshots ORDER BY id").all();
        const events = controlDb.query("SELECT snapshot_id,event_type,actor,principal_kind,principal_id,created_at FROM rm_ownership_snapshot_events ORDER BY id").all();
        const facts = controlDb.query("SELECT fact_id,snapshot_id,fact_hash,canonical_fact,owner_kind,owner_id,owned_company_slug,valid_from,valid_to_exclusive,economic_exact_bp,economic_min_bp,economic_max_bp,voting_bp,control_type,share_class,jurisdiction,review_state,created_at FROM rm_ownership_facts ORDER BY id").all();
        const factEvents = controlDb.query("SELECT fact_hash,event_type,effective_to_exclusive,successor_fact_hash,snapshot_id,actor,principal_kind,principal_id,created_at FROM rm_ownership_fact_events ORDER BY id").all();
        if (snapshots.length > 0 || events.length > 0 || facts.length > 0 || factEvents.length > 0) {
          const path = join(staging, "ownership-graph.json");
          writeFileAtomic(path, `${JSON.stringify({ version: 2, snapshots, events, facts, factEvents })}\n`);
          ownershipGraph = fileEvidence(staging, path);
        }
      } finally { controlDb.close(); }
    }

    const companyEntries: WorkspaceSnapshotManifestV1["companies"] = [];
    for (const company of companies) {
      const companyRoot = companyRootForSlug(workspaceRoot, company.slug);
      const db = openDb(companyPaths(companyRoot).db);
      try {
        migrate(db);
        const backup = createSystemBackup(db, companyRoot, {
          createdAt,
          signWithEd25519: true,
          credentialFree: true,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
        });
        if (!backup.ok || !backup.backupId) throw new Error(backup.errors.join("; "));
        const archivePath = join(staging, "companies", `${company.slug}.tar`);
        mkdirSync(dirname(archivePath), { recursive: true });
        const packed = packBackupArchive(db, companyRoot, {
          backupId: backup.backupId,
          outPath: archivePath,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
        });
        if (!packed.ok) throw new Error(packed.errors.join("; "));
        removePathWithRetry(`${archivePath}.sha256`);
        companyEntries.push({
          slug: company.slug,
          name: company.name,
          archived: company.archived,
          backup: fileEvidence(staging, archivePath),
        });
      } finally { db.close(); }
    }

    const manifest: WorkspaceSnapshotManifestV1 = {
      version: 1,
      createdAt,
      provenance: getReleaseProvenance(),
      credentialPolicy: "omit-all-auth-and-operational-credentials-v1",
      workspaceManifest: fileEvidence(staging, workspaceManifestPath),
      accessPlan: fileEvidence(staging, accessPlanPath),
      ...(companyKnowledge ? { companyKnowledge } : {}),
      ...(ownershipGraph ? { ownershipGraph } : {}),
      ...(workspaceRegistry ? { workspaceRegistry } : {}),
      ...(intercompanyDispositions ? { intercompanyDispositions } : {}),
      ...(workspaceInbox ? { workspaceInbox } : {}),
      companies: companyEntries.sort((a, b) => a.slug.localeCompare(b.slug)),
    };
    writeFileAtomic(join(staging, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    const archive = createTar(dirToTarEntries(staging));
    const digest = sha256(archive);
    mkdirSync(dirname(resolve(outPath)), { recursive: true });
    const temp = writeTempFileFor(outPath, archive);
    promoteTempFileExclusive(temp, outPath);
    const sha256Path = `${outPath}.sha256`;
    writeFileAtomic(sha256Path, `${digest}  ${basename(outPath)}\n`);
    return {
      ok: true,
      snapshotPath: outPath,
      sha256: digest,
      sha256Path,
      companyCount: companyEntries.length,
      accessIdentityCount: accessPlan.users.length,
      appliedRules: [SNAPSHOT_RULE_ID],
      errors: [],
    };
  } catch (error) {
    return { ok: false, appliedRules: [SNAPSHOT_RULE_ID], errors: [`workspace snapshot failed: ${snapshotError(error)}`] };
  } finally {
    removePathWithRetry(staging);
  }
}

function isManifestFile(value: unknown): value is ManifestFile {
  const file = value as ManifestFile;
  return Boolean(file) && typeof file.path === "string" && /^[a-f0-9]{64}$/.test(file.sha256) &&
    Number.isSafeInteger(file.sizeBytes) && file.sizeBytes >= 0;
}

function parseManifest(raw: string): WorkspaceSnapshotManifestV1 | null {
  try {
    const value = JSON.parse(raw) as WorkspaceSnapshotManifestV1;
    if (value.version !== 1 || value.credentialPolicy !== "omit-all-auth-and-operational-credentials-v1" ||
      !validInstant(value.createdAt) || !isReleaseProvenance(value.provenance) ||
      !isManifestFile(value.workspaceManifest) || !isManifestFile(value.accessPlan) ||
      (value.companyKnowledge !== undefined && !isManifestFile(value.companyKnowledge)) ||
      (value.ownershipGraph !== undefined && !isManifestFile(value.ownershipGraph)) ||
      (value.workspaceRegistry !== undefined && !isManifestFile(value.workspaceRegistry)) ||
      (value.intercompanyDispositions !== undefined && !isManifestFile(value.intercompanyDispositions)) ||
      (value.workspaceInbox !== undefined && !isManifestFile(value.workspaceInbox)) ||
      !Array.isArray(value.companies) || value.companies.length === 0) return null;
    const slugs = new Set<string>();
    for (const company of value.companies) {
      if (!/^[a-z0-9][a-z0-9-]*$/.test(company.slug) || !company.name ||
        typeof company.archived !== "boolean" || !isManifestFile(company.backup) || slugs.has(company.slug)) return null;
      slugs.add(company.slug);
    }
    return value;
  } catch { return null; }
}

function verifyFile(root: string, file: ManifestFile): string | null {
  const path = join(root, ...file.path.split("/"));
  if (!existsSync(path) || !statSync(path).isFile()) return `missing snapshot file: ${file.path}`;
  const content = readFileSync(path);
  if (content.byteLength !== file.sizeBytes || sha256(content) !== file.sha256) {
    return `snapshot checksum mismatch: ${file.path}`;
  }
  return null;
}

function parseAccessPlan(raw: string, companies: Set<string>): SnapshotAccessPlan | null {
  try {
    const value = JSON.parse(raw) as SnapshotAccessPlan;
    if (value.version !== 1 || value.recovery !== "bootstrap-owner-then-reinvite" || !Array.isArray(value.users)) return null;
    const emails = new Set<string>();
    for (const user of value.users) {
      const email = user.email?.trim().toLowerCase();
      if (!user.name?.trim() || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) || emails.has(email) ||
        (user.workspaceRole !== "workspace_owner" && user.workspaceRole !== "member") || !Array.isArray(user.memberships)) return null;
      emails.add(email);
      if (user.memberships.some((membership) => !companies.has(membership.companySlug) ||
        !["owner", "bookkeeper", "reviewer", "reader"].includes(membership.role))) return null;
    }
    return value;
  } catch { return null; }
}

function assertCredentialFreeCompanyArchive(archive: Buffer): void {
  for (const entry of readTar(archive)) {
    if (!entry.path.startsWith("config/")) continue;
    const name = entry.path.slice("config/".length);
    if (!SAFE_PORTABLE_CONFIG.has(name)) throw new Error("company snapshot contains non-portable configuration");
  }
}

export function restoreWorkspaceSnapshot(input: {
  snapshotPath: string;
  targetWorkspaceRoot: string;
  createdBy?: string;
  createdByProgram?: string;
}): RestoreWorkspaceSnapshotResult {
  if (!input.snapshotPath || !existsSync(input.snapshotPath) || !statSync(input.snapshotPath).isFile()) {
    return { ok: false, appliedRules: [SNAPSHOT_RULE_ID], errors: ["workspace snapshot does not exist"] };
  }
  const target = resolve(input.targetWorkspaceRoot);
  if (!input.targetWorkspaceRoot || (existsSync(target) && readdirSync(target).length > 0)) {
    return { ok: false, appliedRules: [SNAPSHOT_RULE_ID], errors: ["target workspace must be new or empty"] };
  }
  const extracted = mkdtempSync(join(tmpdir(), "rentemester-workspace-restore-source-"));
  const staging = join(dirname(target), `.restore-workspace-${basename(target)}.${process.pid}.${randomBytes(8).toString("hex")}.tmp`);
  try {
    if (existsSync(staging)) throw new Error("workspace restore staging path already exists");
    const written = extractTar(readFileSync(input.snapshotPath), extracted).sort();
    const manifestPath = join(extracted, "manifest.json");
    const manifest = existsSync(manifestPath) ? parseManifest(readFileSync(manifestPath, "utf8")) : null;
    if (!manifest) throw new Error("workspace snapshot manifest is invalid");
    const expected = ["manifest.json", manifest.workspaceManifest.path, manifest.accessPlan.path, ...(manifest.companyKnowledge ? [manifest.companyKnowledge.path] : []), ...(manifest.ownershipGraph ? [manifest.ownershipGraph.path] : []), ...(manifest.workspaceRegistry ? [manifest.workspaceRegistry.path] : []), ...(manifest.intercompanyDispositions ? [manifest.intercompanyDispositions.path] : []), ...(manifest.workspaceInbox ? [manifest.workspaceInbox.path] : []),
      ...manifest.companies.map((company) => company.backup.path)].sort();
    if (JSON.stringify(written) !== JSON.stringify(expected)) throw new Error("workspace snapshot contains unlisted files");
    for (const file of [manifest.workspaceManifest, manifest.accessPlan, ...(manifest.companyKnowledge ? [manifest.companyKnowledge] : []), ...(manifest.ownershipGraph ? [manifest.ownershipGraph] : []), ...(manifest.workspaceRegistry ? [manifest.workspaceRegistry] : []), ...(manifest.intercompanyDispositions ? [manifest.intercompanyDispositions] : []), ...(manifest.workspaceInbox ? [manifest.workspaceInbox] : []), ...manifest.companies.map((company) => company.backup)]) {
      const error = verifyFile(extracted, file);
      if (error) throw new Error(error);
    }
    const sourceManifest = loadWorkspaceManifest(extracted);
    const declared = new Map(manifest.companies.map((company) => [company.slug, company]));
    if (sourceManifest.companies.length !== declared.size || sourceManifest.companies.some((company) => {
      const match = declared.get(company.slug);
      return !match || match.name !== company.name || match.archived !== company.archived;
    })) throw new Error("workspace and snapshot company manifests disagree");
    const accessPlan = parseAccessPlan(
      readFileSync(join(extracted, ...manifest.accessPlan.path.split("/")), "utf8"),
      new Set(declared.keys()),
    );
    if (!accessPlan) throw new Error("workspace access recovery plan is invalid");

    initWorkspace(staging);
    saveWorkspaceManifest(staging, sourceManifest as WorkspaceManifest);
    if (manifest.workspaceRegistry) {
      const raw = JSON.parse(readFileSync(join(extracted, ...manifest.workspaceRegistry.path.split("/")), "utf8")) as any;
      if (![1,2].includes(raw.version) || ![raw.parties,raw.aliases,raw.assertions,raw.roles,raw.legacyLinks,raw.identifiers,raw.corporate?.bytes,raw.corporate?.events,raw.corporate?.links,raw.corporate?.scopes].every(Array.isArray) || (raw.version===2&&!Array.isArray(raw.legacyMappings))) throw new Error("workspace registry snapshot is invalid");
      const controlDb = openWorkspaceControlDb(staging);
      try { controlDb.transaction(() => {
        for (const row of raw.parties) controlDb.query("INSERT INTO rm_party_events(party_id,event_type,canonical_kind,payload_hash,canonical_payload,actor,created_at) VALUES(?,?,?,?,?,?,?)").run(row.party_id,row.event_type,row.canonical_kind,row.payload_hash,row.canonical_payload,row.actor,row.created_at);
        for (const row of raw.aliases) controlDb.query("INSERT INTO rm_party_alias_assertions(party_id,alias,source,observed_at,review_state,payload_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?)").run(row.party_id,row.alias,row.source,row.observed_at,row.review_state,row.payload_hash,row.actor,row.created_at);
        for (const row of raw.assertions) controlDb.query("INSERT INTO rm_party_field_assertions(party_id,field,value,source,observed_at,review_state,payload_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?)").run(row.party_id,row.field,row.value,row.source,row.observed_at,row.review_state,row.payload_hash,row.actor,row.created_at);
        for (const row of raw.roles) controlDb.query("INSERT INTO rm_party_company_roles(party_id,company_slug,role,defaults_json) VALUES(?,?,?,?)").run(row.party_id,row.company_slug,row.role,row.defaults_json);
        for (const row of raw.legacyLinks) controlDb.query("INSERT INTO rm_party_legacy_links(company_slug,legacy_kind,legacy_id,party_id,actor,created_at) VALUES(?,?,?,?,?,?)").run(row.company_slug,row.legacy_kind,row.legacy_id,row.party_id,row.actor,row.created_at);
        for (const row of raw.legacyMappings??[]) controlDb.query("INSERT INTO rm_legacy_party_mapping_events(company_slug,legacy_kind,legacy_id,party_id,party_role,event_type,version,prior_event_hash,event_hash,contact_snapshot,contact_fingerprint,evidence_json,plan_hash,idempotency_key_hash,idempotency_payload_hash,reason,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").run(row.company_slug,row.legacy_kind,row.legacy_id,row.party_id,row.party_role,row.event_type,row.version,row.prior_event_hash,row.event_hash,row.contact_snapshot,row.contact_fingerprint,row.evidence_json,row.plan_hash,row.idempotency_key_hash,row.idempotency_payload_hash,row.reason,row.actor,row.principal,row.created_at);
        for (const row of raw.identifiers) controlDb.query("INSERT INTO rm_party_identifiers(party_id,jurisdiction,identifier_kind,identifier) VALUES(?,?,?,?)").run(row.party_id,row.jurisdiction,row.identifier_kind,row.identifier);
        for (const row of raw.corporate.bytes) { const body=Buffer.from(String(row.original_bytes_base64),"base64"); if (sha256(body)!==row.sha256) throw new Error("corporate record snapshot hash mismatch"); controlDb.query("INSERT INTO rm_corporate_record_bytes(record_id,original_bytes,sha256,filename,source,received_at,record_date,retention_until,uploader,sensitivity) VALUES(?,?,?,?,?,?,?,?,?,?)").run(row.record_id,body,row.sha256,row.filename,row.source,row.received_at,row.record_date,row.retention_until,row.uploader,row.sensitivity); }
        for (const row of raw.corporate.events) controlDb.query("INSERT INTO rm_corporate_record_events(record_id,event_type,record_type,payload_hash,canonical_payload,actor,created_at) VALUES(?,?,?,?,?,?,?)").run(row.record_id,row.event_type,row.record_type,row.payload_hash,row.canonical_payload,row.actor,row.created_at);
        for (const row of raw.corporate.links) controlDb.query("INSERT INTO rm_corporate_record_links(record_id,link_type,link_id) VALUES(?,?,?)").run(row.record_id,row.link_type,row.link_id);
        for (const row of raw.corporate.scopes) controlDb.query("INSERT INTO rm_corporate_record_scope_assertions(record_id,scope_kind,scope_id,actor,created_at) VALUES(?,?,?,?,?)").run(row.record_id,row.scope_kind,row.scope_id,row.actor,row.created_at);
      })(); } finally { controlDb.close(); }
    }
    if (manifest.intercompanyDispositions) {
      const raw = JSON.parse(readFileSync(join(extracted, ...manifest.intercompanyDispositions.path.split("/")), "utf8")) as any;
      if (raw.version !== 1 || ![raw.dispositions, raw.events, raw.links, raw.lifecycleEvents].every(Array.isArray)) throw new Error("intercompany disposition snapshot is invalid");
      const controlDb = openWorkspaceControlDb(staging);
      try { controlDb.transaction(() => {
        for (const row of raw.dispositions) controlDb.query("INSERT INTO rm_intercompany_dispositions(disposition_id,canonical_payload,payload_hash,created_at) VALUES(?,?,?,?)").run(row.disposition_id,row.canonical_payload,row.payload_hash,row.created_at);
        for (const row of raw.events) controlDb.query("INSERT INTO rm_intercompany_disposition_events(disposition_id,event_type,payload_hash,canonical_payload,actor,principal_kind,principal_id,previous_hash,event_hash,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").run(row.disposition_id,row.event_type,row.payload_hash,row.canonical_payload,row.actor,row.principal_kind,row.principal_id,row.previous_hash,row.event_hash,row.created_at);
        for (const row of raw.links) controlDb.query("INSERT INTO rm_intercompany_disposition_journal_links(disposition_id,side,company_slug,journal_entry_id,journal_entry_no,journal_entry_hash,ledger_head_hash,linked_at,actor,principal_kind,principal_id) VALUES(?,?,?,?,?,?,?,?,?,?,?)").run(row.disposition_id,row.side,row.company_slug,row.journal_entry_id,row.journal_entry_no,row.journal_entry_hash,row.ledger_head_hash,row.linked_at,row.actor,row.principal_kind,row.principal_id);
        for (const row of raw.lifecycleEvents) controlDb.query("INSERT INTO rm_intercompany_disposition_lifecycle_events(disposition_id,event_type,payload_hash,canonical_payload,actor,principal_kind,principal_id,previous_hash,event_hash,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").run(row.disposition_id,row.event_type,row.payload_hash,row.canonical_payload,row.actor,row.principal_kind,row.principal_id,row.previous_hash,row.event_hash,row.created_at);
      })(); } finally { controlDb.close(); }
    }
    if (manifest.workspaceInbox) { const raw=JSON.parse(readFileSync(join(extracted,...manifest.workspaceInbox.path.split("/")),"utf8")) as any;if(raw.version!==1||![raw.sources,raw.events,raw.assignments,raw.exceptions,raw.claims].every(Array.isArray))throw new Error("workspace inbox snapshot is invalid");const db=openWorkspaceControlDb(staging);try{db.transaction(()=>{for(const row of raw.sources){const body=Buffer.from(String(row.original_bytes_base64),"base64");if(sha256(body)!==row.sha256)throw new Error("workspace inbox snapshot hash mismatch");db.query("INSERT INTO rm_workspace_inbox_sources(source_id,visibility_anchor_slug,idempotency_key,original_bytes,sha256,filename,mime_type,transport,transport_identity,received_at,metadata_json,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").run(row.source_id,row.visibility_anchor_slug,row.idempotency_key,body,row.sha256,row.filename,row.mime_type,row.transport,row.transport_identity,row.received_at,row.metadata_json,row.created_by,row.created_at);}for(const row of raw.events)db.query("INSERT INTO rm_workspace_inbox_events(source_id,event_type,payload_hash,canonical_payload,actor,created_at) VALUES(?,?,?,?,?,?)").run(row.source_id,row.event_type,row.payload_hash,row.canonical_payload,row.actor,row.created_at);for(const row of raw.assignments)db.query("INSERT INTO rm_workspace_inbox_assignments(source_id,company_slug,state,document_id,document_no,assigned_by,assigned_at,completed_at) VALUES(?,?,?,?,?,?,?,?)").run(row.source_id,row.company_slug,row.state,row.document_id,row.document_no,row.assigned_by,row.assigned_at,row.completed_at);for(const row of raw.exceptions)db.query("INSERT INTO rm_workspace_inbox_exceptions(source_id,code,required_action,opened_at,resolved_at) VALUES(?,?,?,?,?)").run(row.source_id,row.code,row.required_action,row.opened_at,row.resolved_at);for(const row of raw.claims)db.query("INSERT INTO rm_workspace_inbox_handoff_claims(source_id,company_slug,source_hash,state,claim_id,lease_expires_at,document_id,document_no,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)").run(row.source_id,row.company_slug,row.source_hash,row.state,row.claim_id,row.lease_expires_at,row.document_id,row.document_no,row.created_at,row.updated_at);})()}finally{db.close();}}
    if (manifest.companyKnowledge) {
      const raw = JSON.parse(readFileSync(join(extracted, ...manifest.companyKnowledge.path.split("/")), "utf8")) as { version:number; assertions: any[]; events:any[] };
      if (raw.version !== 1 || !Array.isArray(raw.assertions) || !Array.isArray(raw.events)) throw new Error("workspace knowledge snapshot is invalid");
      const controlDb = openWorkspaceControlDb(staging);
      try { controlDb.transaction(() => {
        for (const row of raw.assertions) controlDb.query("INSERT INTO rm_company_knowledge_assertions(assertion_id,company_slug,predicate,value_json,value_hash,source_kind,source_ref,valid_from,valid_to_exclusive,certainty,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").run(row.assertion_id,row.company_slug,row.predicate,row.value_json,row.value_hash,row.source_kind,row.source_ref,row.valid_from,row.valid_to_exclusive,row.certainty,row.actor,row.principal_kind,row.principal_id,row.created_at);
        for (const row of raw.events) controlDb.query("INSERT INTO rm_company_knowledge_events(assertion_id,event_type,reason,supersedes_assertion_id,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?,?,?)").run(row.assertion_id,row.event_type,row.reason,row.supersedes_assertion_id,row.actor,row.principal_kind,row.principal_id,row.created_at);
      })(); } finally { controlDb.close(); }
    }
    if (manifest.ownershipGraph) {
      const raw = JSON.parse(readFileSync(join(extracted, ...manifest.ownershipGraph.path.split("/")), "utf8")) as { version:number; snapshots:any[]; events:any[]; facts:any[]; factEvents?:any[] };
      if ((raw.version !== 1 && raw.version !== 2) || !Array.isArray(raw.snapshots) || !Array.isArray(raw.events) || !Array.isArray(raw.facts) || (raw.version===2&&!Array.isArray(raw.factEvents))) throw new Error("workspace ownership snapshot is invalid");
      const controlDb = openWorkspaceControlDb(staging);
      try { controlDb.transaction(() => {
        for (const row of raw.snapshots) controlDb.query("INSERT INTO rm_ownership_source_snapshots(snapshot_id,source,observed_at,snapshot_hash,diff_hash,canonical_facts,diff_json,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").run(row.snapshot_id,row.source,row.observed_at,row.snapshot_hash,row.diff_hash,row.canonical_facts,row.diff_json,row.actor,row.principal_kind,row.principal_id,row.created_at);
        for (const row of raw.events) controlDb.query("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?)").run(row.snapshot_id,row.event_type,row.actor,row.principal_kind,row.principal_id,row.created_at);
        for (const row of raw.facts) controlDb.query("INSERT INTO rm_ownership_facts(fact_id,snapshot_id,fact_hash,canonical_fact,owner_kind,owner_id,owned_company_slug,valid_from,valid_to_exclusive,economic_exact_bp,economic_min_bp,economic_max_bp,voting_bp,control_type,share_class,jurisdiction,review_state,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").run(row.fact_id,row.snapshot_id,row.fact_hash,row.canonical_fact,row.owner_kind,row.owner_id,row.owned_company_slug,row.valid_from,row.valid_to_exclusive,row.economic_exact_bp,row.economic_min_bp,row.economic_max_bp,row.voting_bp,row.control_type,row.share_class,row.jurisdiction,row.review_state,row.created_at);
        for (const row of raw.factEvents??[]) controlDb.query("INSERT INTO rm_ownership_fact_events(fact_hash,event_type,effective_to_exclusive,successor_fact_hash,snapshot_id,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)").run(row.fact_hash,row.event_type,row.effective_to_exclusive,row.successor_fact_hash,row.snapshot_id,row.actor,row.principal_kind,row.principal_id,row.created_at);
      })(); } finally { controlDb.close(); }
    }
    for (const company of manifest.companies) {
      const archivePath = join(extracted, ...company.backup.path.split("/"));
      const archive = readFileSync(archivePath);
      assertCredentialFreeCompanyArchive(archive);
      const companySource = mkdtempSync(join(tmpdir(), `rentemester-company-restore-${company.slug}-`));
      try {
        extractTar(archive, companySource);
        const publicKeyPath = join(companySource, "config", "backup-manifest.pub");
        if (!existsSync(publicKeyPath)) throw new Error("company snapshot is missing its verification key");
        const restored = restoreSystemBackup({
          backupDir: companySource,
          targetCompanyRoot: companyRootForSlug(staging, company.slug),
          publicKeyPath,
          credentialFreePortableMode: true,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
        });
        if (!restored.ok) throw new Error(restored.errors.join("; "));
      } finally { removePathWithRetry(companySource); }
    }
    const recoveryDir = join(staging, ".rentemester");
    mkdirSync(recoveryDir, { recursive: true });
    const recoveryPath = join(recoveryDir, "restored-access-plan.json");
    writeFileAtomic(recoveryPath, `${JSON.stringify(accessPlan, null, 2)}\n`);
    chmodSync(recoveryPath, 0o600);
    if (existsSync(target)) removePathWithRetry(target);
    else mkdirSync(dirname(target), { recursive: true });
    renamePathWithRetry(staging, target);
    return {
      ok: true,
      targetWorkspaceRoot: target,
      companyCount: manifest.companies.length,
      accessRecoveryPlanPath: join(target, ".rentemester", "restored-access-plan.json"),
      nextStep: "bootstrap-owner-then-reinvite",
      appliedRules: [SNAPSHOT_RULE_ID],
      errors: [],
    };
  } catch (error) {
    if (existsSync(staging)) removePathWithRetry(staging);
    return { ok: false, appliedRules: [SNAPSHOT_RULE_ID], errors: [`workspace restore failed: ${snapshotError(error)}`] };
  } finally {
    removePathWithRetry(extracted);
  }
}
