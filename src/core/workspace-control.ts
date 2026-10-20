import { assertLegacySourceActive } from "./legacy-retirement";
import { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { resolveActor, type ResolveActorInput } from "./actor";
import { getBuildIdentity } from "./build-identity";
import { openSqliteReadOnlySnapshot } from "./sqlite-readonly-snapshot";

/** Private workspace-owned state. Company ledgers are deliberately elsewhere. */
export const WORKSPACE_CONTROL_DIRECTORY = ".rentemester";
export const WORKSPACE_CONTROL_DB_FILE = "workspace-control.sqlite";
export const WORKSPACE_CONTROL_BASELINE_MIGRATION_ID = 1;
export const WORKSPACE_CONTROL_BASELINE_MIGRATION_NAME =
  "rentemester-workspace-control-baseline-v1";
export const WORKSPACE_CONTROL_BETTER_AUTH_MIGRATION_ID = 2;
export const WORKSPACE_CONTROL_BETTER_AUTH_MIGRATION_NAME =
  "rentemester-better-auth-foundation-v2";
export const WORKSPACE_CONTROL_ACCESS_MIGRATION_ID = 3;
export const WORKSPACE_CONTROL_ACCESS_MIGRATION_NAME =
  "rentemester-workspace-access-events-v3";
export const WORKSPACE_CONTROL_BOOTSTRAP_MIGRATION_ID = 4;
export const WORKSPACE_CONTROL_BOOTSTRAP_MIGRATION_NAME =
  "rentemester-workspace-bootstrap-saga-v4";
export const WORKSPACE_CONTROL_SESSION_ASSURANCE_MIGRATION_ID = 5;
export const WORKSPACE_CONTROL_SESSION_ASSURANCE_MIGRATION_NAME =
  "rentemester-session-assurance-v5";
export const WORKSPACE_CONTROL_AUTH_AUDIT_MIGRATION_ID = 6;
export const WORKSPACE_CONTROL_AUTH_AUDIT_MIGRATION_NAME = "rentemester-auth-evidence-v6";
export const WORKSPACE_CONTROL_GROUP_STRUCTURE_MIGRATION_ID = 7;
export const WORKSPACE_CONTROL_GROUP_STRUCTURE_MIGRATION_NAME = "rentemester-group-structure-v7";
export const WORKSPACE_CONTROL_DOCUMENT_ACCESS_MIGRATION_ID = 8;
export const WORKSPACE_CONTROL_DOCUMENT_ACCESS_MIGRATION_NAME = "rentemester-document-access-audit-v8";
export const WORKSPACE_CONTROL_INTERCOMPANY_MAPPING_MIGRATION_ID = 9;
export const WORKSPACE_CONTROL_INTERCOMPANY_MAPPING_MIGRATION_NAME = "rentemester-intercompany-mapping-v9";
export const WORKSPACE_CONTROL_CONSOLIDATION_ELIMINATION_MIGRATION_ID = 10;
export const WORKSPACE_CONTROL_CONSOLIDATION_ELIMINATION_MIGRATION_NAME = "rentemester-consolidation-elimination-v10";
export const WORKSPACE_CONTROL_CONSOLIDATION_PROFILE_MIGRATION_ID = 11;
export const WORKSPACE_CONTROL_CONSOLIDATION_PROFILE_MIGRATION_NAME = "rentemester-consolidation-profile-v11";
export const WORKSPACE_CONTROL_AUTH_RECOVERY_TELEMETRY_MIGRATION_ID = 12;
export const WORKSPACE_CONTROL_AUTH_RECOVERY_TELEMETRY_MIGRATION_NAME = "rentemester-auth-recovery-telemetry-v12";
export const WORKSPACE_CONTROL_AUTHORIZATION_DENIAL_AUDIT_MIGRATION_ID = 13;
export const WORKSPACE_CONTROL_AUTHORIZATION_DENIAL_AUDIT_MIGRATION_NAME = "rentemester-authorization-denial-audit-v13";
export const WORKSPACE_CONTROL_INVITATIONS_MIGRATION_ID = 14;
export const WORKSPACE_CONTROL_INVITATIONS_MIGRATION_NAME = "rentemester-workspace-invitations-v14";
export const WORKSPACE_CONTROL_PARTY_REGISTRY_MIGRATION_ID = 15;
export const WORKSPACE_CONTROL_PARTY_REGISTRY_MIGRATION_NAME = "rentemester-party-registry-v15";
export const WORKSPACE_CONTROL_CORPORATE_RECORDS_MIGRATION_ID = 16;
export const WORKSPACE_CONTROL_CORPORATE_RECORDS_MIGRATION_NAME = "rentemester-corporate-records-v16";
export const WORKSPACE_CONTROL_SERVICE_PRINCIPALS_MIGRATION_ID = 17;
export const WORKSPACE_CONTROL_SERVICE_PRINCIPALS_MIGRATION_NAME = "rentemester-workspace-service-principals-v17";
export const WORKSPACE_CONTROL_SERVICE_PRINCIPAL_SAGA_MIGRATION_ID = 18;
export const WORKSPACE_CONTROL_SERVICE_PRINCIPAL_SAGA_MIGRATION_NAME = "rentemester-workspace-service-principal-saga-v18";
export const WORKSPACE_CONTROL_PARTY_ASSERTIONS_MIGRATION_ID = 19;
export const WORKSPACE_CONTROL_PARTY_ASSERTIONS_MIGRATION_NAME = "rentemester-party-assertions-v19";
export const WORKSPACE_CONTROL_CORPORATE_RECORD_GOVERNANCE_MIGRATION_ID = 20;
export const WORKSPACE_CONTROL_CORPORATE_RECORD_GOVERNANCE_MIGRATION_NAME = "rentemester-corporate-record-governance-v20";
export const WORKSPACE_CONTROL_COMPANY_KNOWLEDGE_MIGRATION_ID = 21;
export const WORKSPACE_CONTROL_COMPANY_KNOWLEDGE_MIGRATION_NAME = "rentemester-company-knowledge-v21";
export const WORKSPACE_CONTROL_OWNERSHIP_GRAPH_MIGRATION_ID = 22;
export const WORKSPACE_CONTROL_OWNERSHIP_GRAPH_MIGRATION_NAME = "rentemester-ownership-graph-v22";
export const WORKSPACE_CONTROL_CORPORATE_RECORD_SCOPE_INTEGRITY_MIGRATION_ID = 23;
export const WORKSPACE_CONTROL_CORPORATE_RECORD_SCOPE_INTEGRITY_MIGRATION_NAME = "rentemester-corporate-record-scope-integrity-v23";
export const WORKSPACE_CONTROL_OWNERSHIP_FACT_LIFECYCLE_MIGRATION_ID = 24;
export const WORKSPACE_CONTROL_OWNERSHIP_FACT_LIFECYCLE_MIGRATION_NAME = "rentemester-ownership-fact-lifecycle-v24";
export const WORKSPACE_CONTROL_DOCUMENT_INBOX_MIGRATION_ID = 25;
export const WORKSPACE_CONTROL_DOCUMENT_INBOX_MIGRATION_NAME = "rentemester-workspace-document-inbox-v25";
export const WORKSPACE_CONTROL_DOCUMENT_INBOX_HANDOFF_CLAIMS_MIGRATION_ID = 26;
export const WORKSPACE_CONTROL_DOCUMENT_INBOX_HANDOFF_CLAIMS_MIGRATION_NAME = "rentemester-workspace-inbox-handoff-claims-v26";
export const WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITIONS_MIGRATION_ID = 27;
export const WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITIONS_MIGRATION_NAME = "rentemester-intercompany-dispositions-v27";
export const WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITION_LIFECYCLE_MIGRATION_ID = 28;
export const WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITION_LIFECYCLE_MIGRATION_NAME = "rentemester-intercompany-disposition-lifecycle-v28";

const baselineArtifact = readFileSync(
  join(import.meta.dir, "workspace-migrations", "0001-workspace-control-baseline.json"),
);
const betterAuthArtifact = readFileSync(
  join(import.meta.dir, "workspace-migrations", "0002-better-auth-foundation.json"),
);
const workspaceAccessArtifact = readFileSync(
  join(import.meta.dir, "workspace-migrations", "0003-workspace-access-events.json"),
);
const workspaceBootstrapArtifact = readFileSync(
  join(import.meta.dir, "workspace-migrations", "0004-workspace-bootstrap-saga-v4.json"),
);
const workspaceSessionAssuranceArtifact = readFileSync(
  join(import.meta.dir, "workspace-migrations", "0005-session-assurance-v5.json"),
);
const workspaceAuthAuditArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0006-auth-audit-v6.json"));
const workspaceGroupStructureArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0007-group-structure-v7.json"));
const workspaceDocumentAccessArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0008-document-access-audit-v8.json"));
const workspaceIntercompanyMappingArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0009-intercompany-mapping-v9.json"));
const workspaceConsolidationEliminationArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0010-consolidation-elimination-v10.json"));
const workspaceConsolidationProfileArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0011-consolidation-profile-v11.json"));
const workspaceAuthRecoveryTelemetryArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0012-auth-recovery-telemetry-v12.json"));
const workspaceAuthorizationDenialAuditArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0013-authorization-denial-audit-v13.json"));
const workspaceInvitationsArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0014-workspace-invitations-v14.json"));
const workspacePartyRegistryArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0015-party-registry-v15.json"));
const workspaceCorporateRecordsArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0016-corporate-records-v16.json"));
const workspaceServicePrincipalsArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0017-service-principals-v17.json"));
const workspaceServicePrincipalSagaArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0018-service-principal-saga-v18.json"));
const workspacePartyAssertionsArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0019-party-assertions-v19.json"));
const workspaceCorporateRecordGovernanceArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0020-corporate-record-governance-v20.json"));
const workspaceCompanyKnowledgeArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0021-company-knowledge-v21.json"));
const workspaceOwnershipGraphArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0022-ownership-graph-v22.json"));
const workspaceCorporateRecordScopeIntegrityArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0023-corporate-record-scope-integrity-v23.json"));
const workspaceOwnershipFactLifecycleArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0024-ownership-fact-lifecycle-v24.json"));
const workspaceDocumentInboxArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0025-workspace-document-inbox-v25.json"));
const workspaceDocumentInboxHandoffClaimsArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0026-workspace-inbox-handoff-claims-v26.json"));
const workspaceIntercompanyDispositionsArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0027-intercompany-dispositions-v27.json"));
const workspaceIntercompanyDispositionLifecycleArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0028-intercompany-disposition-lifecycle-v28.json"));
const workspaceInboxSingleAssignmentArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0029-workspace-inbox-single-assignment-v29.json"));
const workspaceDocumentPartyRoleTaxonomyArtifact = readFileSync(join(import.meta.dir, "workspace-migrations", "0030-document-party-role-taxonomy-v30.json"));

export const WORKSPACE_CONTROL_BASELINE_MIGRATION_CHECKSUM = createHash("sha256")
  .update(baselineArtifact)
  .digest("hex");
export const WORKSPACE_CONTROL_BETTER_AUTH_MIGRATION_CHECKSUM = createHash("sha256")
  .update(betterAuthArtifact)
  .digest("hex");
export const WORKSPACE_CONTROL_ACCESS_MIGRATION_CHECKSUM = createHash("sha256")
  .update(workspaceAccessArtifact)
  .digest("hex");
export const WORKSPACE_CONTROL_BOOTSTRAP_MIGRATION_CHECKSUM = createHash("sha256")
  .update(workspaceBootstrapArtifact)
  .digest("hex");
export const WORKSPACE_CONTROL_SESSION_ASSURANCE_MIGRATION_CHECKSUM = createHash("sha256")
  .update(workspaceSessionAssuranceArtifact)
  .digest("hex");
export const WORKSPACE_CONTROL_AUTH_AUDIT_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceAuthAuditArtifact).digest("hex");
export const WORKSPACE_CONTROL_GROUP_STRUCTURE_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceGroupStructureArtifact).digest("hex");
export const WORKSPACE_CONTROL_DOCUMENT_ACCESS_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceDocumentAccessArtifact).digest("hex");
export const WORKSPACE_CONTROL_INTERCOMPANY_MAPPING_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceIntercompanyMappingArtifact).digest("hex");
export const WORKSPACE_CONTROL_CONSOLIDATION_ELIMINATION_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceConsolidationEliminationArtifact).digest("hex");
export const WORKSPACE_CONTROL_CONSOLIDATION_PROFILE_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceConsolidationProfileArtifact).digest("hex");
export const WORKSPACE_CONTROL_AUTH_RECOVERY_TELEMETRY_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceAuthRecoveryTelemetryArtifact).digest("hex");
export const WORKSPACE_CONTROL_AUTHORIZATION_DENIAL_AUDIT_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceAuthorizationDenialAuditArtifact).digest("hex");
export const WORKSPACE_CONTROL_INVITATIONS_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceInvitationsArtifact).digest("hex");
export const WORKSPACE_CONTROL_PARTY_REGISTRY_MIGRATION_CHECKSUM = createHash("sha256").update(workspacePartyRegistryArtifact).digest("hex");
export const WORKSPACE_CONTROL_CORPORATE_RECORDS_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceCorporateRecordsArtifact).digest("hex");
export const WORKSPACE_CONTROL_SERVICE_PRINCIPALS_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceServicePrincipalsArtifact).digest("hex");
export const WORKSPACE_CONTROL_SERVICE_PRINCIPAL_SAGA_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceServicePrincipalSagaArtifact).digest("hex");
export const WORKSPACE_CONTROL_PARTY_ASSERTIONS_MIGRATION_CHECKSUM = createHash("sha256").update(workspacePartyAssertionsArtifact).digest("hex");
export const WORKSPACE_CONTROL_CORPORATE_RECORD_GOVERNANCE_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceCorporateRecordGovernanceArtifact).digest("hex");
export const WORKSPACE_CONTROL_COMPANY_KNOWLEDGE_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceCompanyKnowledgeArtifact).digest("hex");
export const WORKSPACE_CONTROL_OWNERSHIP_GRAPH_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceOwnershipGraphArtifact).digest("hex");
export const WORKSPACE_CONTROL_CORPORATE_RECORD_SCOPE_INTEGRITY_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceCorporateRecordScopeIntegrityArtifact).digest("hex");
export const WORKSPACE_CONTROL_OWNERSHIP_FACT_LIFECYCLE_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceOwnershipFactLifecycleArtifact).digest("hex");
export const WORKSPACE_CONTROL_DOCUMENT_INBOX_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceDocumentInboxArtifact).digest("hex");
export const WORKSPACE_CONTROL_DOCUMENT_INBOX_HANDOFF_CLAIMS_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceDocumentInboxHandoffClaimsArtifact).digest("hex");
export const WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITIONS_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceIntercompanyDispositionsArtifact).digest("hex");
export const WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITION_LIFECYCLE_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceIntercompanyDispositionLifecycleArtifact).digest("hex");
export const WORKSPACE_CONTROL_INBOX_SINGLE_ASSIGNMENT_MIGRATION_ID = 29;
export const WORKSPACE_CONTROL_INBOX_SINGLE_ASSIGNMENT_MIGRATION_NAME = "rentemester-workspace-inbox-single-assignment-v29";
export const WORKSPACE_CONTROL_INBOX_SINGLE_ASSIGNMENT_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceInboxSingleAssignmentArtifact).digest("hex");
export const WORKSPACE_CONTROL_DOCUMENT_PARTY_ROLE_TAXONOMY_MIGRATION_ID = 30;
export const WORKSPACE_CONTROL_DOCUMENT_PARTY_ROLE_TAXONOMY_MIGRATION_NAME = "rentemester-document-party-role-taxonomy-v30";
export const WORKSPACE_CONTROL_DOCUMENT_PARTY_ROLE_TAXONOMY_MIGRATION_CHECKSUM = createHash("sha256").update(workspaceDocumentPartyRoleTaxonomyArtifact).digest("hex");

export type WorkspaceControlPaths = {
  root: string;
  directory: string;
  db: string;
};

export function workspaceControlPaths(workspaceRoot: string): WorkspaceControlPaths {
  const directory = join(workspaceRoot, WORKSPACE_CONTROL_DIRECTORY);
  return { root: workspaceRoot, directory, db: join(directory, WORKSPACE_CONTROL_DB_FILE) };
}

type WorkspaceMigration = {
  id: number;
  name: string;
  checksum: string;
  artifact: Buffer;
};

export type WorkspaceMigrationRow = {
  id: number;
  name: string;
  checksum: string | null;
};

const migrations: readonly WorkspaceMigration[] = [
  {
    id: WORKSPACE_CONTROL_BASELINE_MIGRATION_ID,
    name: WORKSPACE_CONTROL_BASELINE_MIGRATION_NAME,
    checksum: WORKSPACE_CONTROL_BASELINE_MIGRATION_CHECKSUM,
    artifact: baselineArtifact,
  },
  {
    id: WORKSPACE_CONTROL_BETTER_AUTH_MIGRATION_ID,
    name: WORKSPACE_CONTROL_BETTER_AUTH_MIGRATION_NAME,
    checksum: WORKSPACE_CONTROL_BETTER_AUTH_MIGRATION_CHECKSUM,
    artifact: betterAuthArtifact,
  },
  {
    id: WORKSPACE_CONTROL_ACCESS_MIGRATION_ID,
    name: WORKSPACE_CONTROL_ACCESS_MIGRATION_NAME,
    checksum: WORKSPACE_CONTROL_ACCESS_MIGRATION_CHECKSUM,
    artifact: workspaceAccessArtifact,
  },
  {
    id: WORKSPACE_CONTROL_BOOTSTRAP_MIGRATION_ID,
    name: WORKSPACE_CONTROL_BOOTSTRAP_MIGRATION_NAME,
    checksum: WORKSPACE_CONTROL_BOOTSTRAP_MIGRATION_CHECKSUM,
    artifact: workspaceBootstrapArtifact,
  },
  {
    id: WORKSPACE_CONTROL_SESSION_ASSURANCE_MIGRATION_ID,
    name: WORKSPACE_CONTROL_SESSION_ASSURANCE_MIGRATION_NAME,
    checksum: WORKSPACE_CONTROL_SESSION_ASSURANCE_MIGRATION_CHECKSUM,
    artifact: workspaceSessionAssuranceArtifact,
  },
  { id: WORKSPACE_CONTROL_AUTH_AUDIT_MIGRATION_ID, name: WORKSPACE_CONTROL_AUTH_AUDIT_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_AUTH_AUDIT_MIGRATION_CHECKSUM, artifact: workspaceAuthAuditArtifact },
  { id: WORKSPACE_CONTROL_GROUP_STRUCTURE_MIGRATION_ID, name: WORKSPACE_CONTROL_GROUP_STRUCTURE_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_GROUP_STRUCTURE_MIGRATION_CHECKSUM, artifact: workspaceGroupStructureArtifact },
  { id: WORKSPACE_CONTROL_DOCUMENT_ACCESS_MIGRATION_ID, name: WORKSPACE_CONTROL_DOCUMENT_ACCESS_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_DOCUMENT_ACCESS_MIGRATION_CHECKSUM, artifact: workspaceDocumentAccessArtifact },
  { id: WORKSPACE_CONTROL_INTERCOMPANY_MAPPING_MIGRATION_ID, name: WORKSPACE_CONTROL_INTERCOMPANY_MAPPING_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_INTERCOMPANY_MAPPING_MIGRATION_CHECKSUM, artifact: workspaceIntercompanyMappingArtifact },
  { id: WORKSPACE_CONTROL_CONSOLIDATION_ELIMINATION_MIGRATION_ID, name: WORKSPACE_CONTROL_CONSOLIDATION_ELIMINATION_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_CONSOLIDATION_ELIMINATION_MIGRATION_CHECKSUM, artifact: workspaceConsolidationEliminationArtifact },
  { id: WORKSPACE_CONTROL_CONSOLIDATION_PROFILE_MIGRATION_ID, name: WORKSPACE_CONTROL_CONSOLIDATION_PROFILE_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_CONSOLIDATION_PROFILE_MIGRATION_CHECKSUM, artifact: workspaceConsolidationProfileArtifact },
  { id: WORKSPACE_CONTROL_AUTH_RECOVERY_TELEMETRY_MIGRATION_ID, name: WORKSPACE_CONTROL_AUTH_RECOVERY_TELEMETRY_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_AUTH_RECOVERY_TELEMETRY_MIGRATION_CHECKSUM, artifact: workspaceAuthRecoveryTelemetryArtifact },
  { id: WORKSPACE_CONTROL_AUTHORIZATION_DENIAL_AUDIT_MIGRATION_ID, name: WORKSPACE_CONTROL_AUTHORIZATION_DENIAL_AUDIT_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_AUTHORIZATION_DENIAL_AUDIT_MIGRATION_CHECKSUM, artifact: workspaceAuthorizationDenialAuditArtifact },
  { id: WORKSPACE_CONTROL_INVITATIONS_MIGRATION_ID, name: WORKSPACE_CONTROL_INVITATIONS_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_INVITATIONS_MIGRATION_CHECKSUM, artifact: workspaceInvitationsArtifact },
  { id: WORKSPACE_CONTROL_PARTY_REGISTRY_MIGRATION_ID, name: WORKSPACE_CONTROL_PARTY_REGISTRY_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_PARTY_REGISTRY_MIGRATION_CHECKSUM, artifact: workspacePartyRegistryArtifact },
  { id: WORKSPACE_CONTROL_CORPORATE_RECORDS_MIGRATION_ID, name: WORKSPACE_CONTROL_CORPORATE_RECORDS_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_CORPORATE_RECORDS_MIGRATION_CHECKSUM, artifact: workspaceCorporateRecordsArtifact },
  { id: WORKSPACE_CONTROL_SERVICE_PRINCIPALS_MIGRATION_ID, name: WORKSPACE_CONTROL_SERVICE_PRINCIPALS_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_SERVICE_PRINCIPALS_MIGRATION_CHECKSUM, artifact: workspaceServicePrincipalsArtifact },
  { id: WORKSPACE_CONTROL_SERVICE_PRINCIPAL_SAGA_MIGRATION_ID, name: WORKSPACE_CONTROL_SERVICE_PRINCIPAL_SAGA_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_SERVICE_PRINCIPAL_SAGA_MIGRATION_CHECKSUM, artifact: workspaceServicePrincipalSagaArtifact },
  { id: WORKSPACE_CONTROL_PARTY_ASSERTIONS_MIGRATION_ID, name: WORKSPACE_CONTROL_PARTY_ASSERTIONS_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_PARTY_ASSERTIONS_MIGRATION_CHECKSUM, artifact: workspacePartyAssertionsArtifact },
  { id: WORKSPACE_CONTROL_CORPORATE_RECORD_GOVERNANCE_MIGRATION_ID, name: WORKSPACE_CONTROL_CORPORATE_RECORD_GOVERNANCE_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_CORPORATE_RECORD_GOVERNANCE_MIGRATION_CHECKSUM, artifact: workspaceCorporateRecordGovernanceArtifact },
  { id: WORKSPACE_CONTROL_COMPANY_KNOWLEDGE_MIGRATION_ID, name: WORKSPACE_CONTROL_COMPANY_KNOWLEDGE_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_COMPANY_KNOWLEDGE_MIGRATION_CHECKSUM, artifact: workspaceCompanyKnowledgeArtifact },
  { id: WORKSPACE_CONTROL_OWNERSHIP_GRAPH_MIGRATION_ID, name: WORKSPACE_CONTROL_OWNERSHIP_GRAPH_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_OWNERSHIP_GRAPH_MIGRATION_CHECKSUM, artifact: workspaceOwnershipGraphArtifact },
  { id: WORKSPACE_CONTROL_CORPORATE_RECORD_SCOPE_INTEGRITY_MIGRATION_ID, name: WORKSPACE_CONTROL_CORPORATE_RECORD_SCOPE_INTEGRITY_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_CORPORATE_RECORD_SCOPE_INTEGRITY_MIGRATION_CHECKSUM, artifact: workspaceCorporateRecordScopeIntegrityArtifact },
  { id: WORKSPACE_CONTROL_OWNERSHIP_FACT_LIFECYCLE_MIGRATION_ID, name: WORKSPACE_CONTROL_OWNERSHIP_FACT_LIFECYCLE_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_OWNERSHIP_FACT_LIFECYCLE_MIGRATION_CHECKSUM, artifact: workspaceOwnershipFactLifecycleArtifact },
  { id: WORKSPACE_CONTROL_DOCUMENT_INBOX_MIGRATION_ID, name: WORKSPACE_CONTROL_DOCUMENT_INBOX_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_DOCUMENT_INBOX_MIGRATION_CHECKSUM, artifact: workspaceDocumentInboxArtifact },
  { id: WORKSPACE_CONTROL_DOCUMENT_INBOX_HANDOFF_CLAIMS_MIGRATION_ID, name: WORKSPACE_CONTROL_DOCUMENT_INBOX_HANDOFF_CLAIMS_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_DOCUMENT_INBOX_HANDOFF_CLAIMS_MIGRATION_CHECKSUM, artifact: workspaceDocumentInboxHandoffClaimsArtifact },
  { id: WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITIONS_MIGRATION_ID, name: WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITIONS_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITIONS_MIGRATION_CHECKSUM, artifact: workspaceIntercompanyDispositionsArtifact },
  { id: WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITION_LIFECYCLE_MIGRATION_ID, name: WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITION_LIFECYCLE_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_INTERCOMPANY_DISPOSITION_LIFECYCLE_MIGRATION_CHECKSUM, artifact: workspaceIntercompanyDispositionLifecycleArtifact },
  { id: WORKSPACE_CONTROL_INBOX_SINGLE_ASSIGNMENT_MIGRATION_ID, name: WORKSPACE_CONTROL_INBOX_SINGLE_ASSIGNMENT_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_INBOX_SINGLE_ASSIGNMENT_MIGRATION_CHECKSUM, artifact: workspaceInboxSingleAssignmentArtifact },
  { id: WORKSPACE_CONTROL_DOCUMENT_PARTY_ROLE_TAXONOMY_MIGRATION_ID, name: WORKSPACE_CONTROL_DOCUMENT_PARTY_ROLE_TAXONOMY_MIGRATION_NAME, checksum: WORKSPACE_CONTROL_DOCUMENT_PARTY_ROLE_TAXONOMY_MIGRATION_CHECKSUM, artifact: workspaceDocumentPartyRoleTaxonomyArtifact },
];

/** Latest immutable control-schema migration this runtime can safely serve. */
export const CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION = migrations.at(-1)!.id;

function tableExists(db: Database, name: string): boolean {
  return db.query(
    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?",
  ).get(name) != null;
}

function triggerExists(db: Database, name: string): boolean {
  return db.query(
    "SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = ?",
  ).get(name) != null;
}

function objectSql(db: Database, type: "table" | "trigger" | "index", name: string): string | null {
  const row = db.query("SELECT sql FROM sqlite_master WHERE type = ? AND name = ?").get(type, name) as { sql: string | null } | null;
  return row?.sql ?? null;
}

function assertInvitationSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_workspace_invitation_events)").all() as Array<{
    name: string;
    notnull: number;
    pk: number;
  }>;
  const expected = [
    "id", "invitation_id", "event_type", "token_hash", "token_key_version",
    "canonical_email", "email_hash", "workspace_role", "company_slug",
    "company_role", "expires_at", "user_id", "actor", "created_at",
  ];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) {
    throw new Error("workspace invitation table has an unsupported schema");
  }
  for (const name of ["invitation_id", "event_type", "actor", "created_at"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) {
      throw new Error("workspace invitation table requires immutable event fields");
    }
  }
  if (columns.find((column) => column.name === "id")?.pk !== 1) {
    throw new Error("workspace invitation table requires a primary event id");
  }
  const tableSql = objectSql(db, "table", "rm_workspace_invitation_events")
    ?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const required of [
    "event_type in ('issued','delivery_confirmed','delivery_failed','accepted','cancelled')",
    "event_type='accepted'",
    "length(token_hash)=64",
    "canonical_email = lower(trim(canonical_email))",
  ]) {
    if (!tableSql.includes(required)) throw new Error("workspace invitation table has unsupported constraints");
  }
  const tokenIndex = objectSql(db, "index", "rm_workspace_invitation_events_token_uidx")
    ?.replace(/\s+/g, " ").toLowerCase() ?? "";
  const acceptedIndex = objectSql(db, "index", "rm_workspace_invitation_events_accepted_uidx")
    ?.replace(/\s+/g, " ").toLowerCase() ?? "";
  if (!tokenIndex.includes("unique index") || !tokenIndex.includes("where token_hash is not null") ||
    !acceptedIndex.includes("unique index") || !acceptedIndex.includes("where event_type='accepted'")) {
    throw new Error("workspace invitation table lacks replay protection");
  }
}

function assertGroupManifestSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_group_manifest_events)").all() as Array<{ name: string; type: string; notnull: number; pk: number }>;
  const expected = ["id", "manifest_hash", "previous_hash", "canonical_manifest", "actor", "created_at"];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) {
    throw new Error("workspace group structure table has an unsupported schema");
  }
  for (const name of ["manifest_hash", "canonical_manifest", "actor", "created_at"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) throw new Error("workspace group structure table requires immutable event fields");
  }
  if (columns.find((column) => column.name === "id")?.pk !== 1) throw new Error("workspace group structure table requires a primary event id");
  const tableSql = objectSql(db, "table", "rm_group_manifest_events")?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const requiredCheck of [
    "check(id > 0)",
    "check(length(manifest_hash)=64 and manifest_hash not glob '*[^0-9a-f]*')",
    "check(previous_hash is null or (length(previous_hash)=64 and previous_hash not glob '*[^0-9a-f]*'))",
    "check(length(canonical_manifest) between 2 and 262144)",
    "check(length(actor) between 3 and 160)",
    "check(length(created_at) between 19 and 30)",
  ]) {
    if (!tableSql.includes(requiredCheck)) throw new Error("workspace group structure table has unsupported constraints");
  }
  for (const [trigger, operation] of [["rm_group_manifest_events_no_update", "UPDATE"], ["rm_group_manifest_events_no_delete", "DELETE"]] as const) {
    const sql = objectSql(db, "trigger", trigger);
    if (!sql || !new RegExp(`BEFORE\\s+${operation}\\s+ON\\s+rm_group_manifest_events`, "i").test(sql) || !/RAISE\s*\(\s*ABORT\s*,\s*'group manifest events are append-only'\s*\)/i.test(sql)) {
      throw new Error("workspace group structure table is not append-only");
    }
  }
}

function assertIntercompanyMappingSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_intercompany_mapping_events)").all() as Array<{ name: string; notnull: number; pk: number }>;
  const expected = ["id", "mapping_id", "event_type", "mapping_hash", "canonical_mapping", "previous_hash", "event_hash", "actor", "created_at"];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) throw new Error("workspace intercompany mapping table has an unsupported schema");
  for (const name of ["mapping_id", "event_type", "mapping_hash", "canonical_mapping", "event_hash", "actor", "created_at"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) throw new Error("workspace intercompany mapping table requires immutable event fields");
  }
  if (columns.find((column) => column.name === "id")?.pk !== 1) throw new Error("workspace intercompany mapping table requires a primary event id");
  const sql = objectSql(db, "table", "rm_intercompany_mapping_events")?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const required of ["check(id > 0)", "check(event_type in ('proposed','approved','revoked'))", "check(length(mapping_hash)=64", "check(length(event_hash)=64", "check(length(canonical_mapping) between 2 and 65536)"]) {
    if (!sql.includes(required)) throw new Error("workspace intercompany mapping table has unsupported constraints");
  }
}

function assertConsolidationEliminationSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_consolidation_elimination_events)").all() as Array<{ name: string; notnull: number; pk: number }>;
  const expected = ["id", "elimination_id", "event_type", "payload_hash", "canonical_payload", "previous_hash", "event_hash", "actor", "created_at"];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) throw new Error("workspace consolidation elimination table has an unsupported schema");
  for (const name of ["elimination_id", "event_type", "payload_hash", "canonical_payload", "event_hash", "actor", "created_at"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) throw new Error("workspace consolidation elimination table requires immutable event fields");
  }
  const sql = objectSql(db, "table", "rm_consolidation_elimination_events")?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const required of ["check(id > 0)", "check(event_type in ('proposed','approved','rejected','applied','reversed'))", "check(length(payload_hash)=64", "check(length(event_hash)=64", "check(length(canonical_payload) between 2 and 262144)"]) {
    if (!sql.includes(required)) throw new Error("workspace consolidation elimination table has unsupported constraints");
  }
}

function assertConsolidationProfileSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_consolidation_profile_events)").all() as Array<{ name: string; notnull: number; pk: number }>;
  const expected = ["id", "profile_id", "event_type", "profile_hash", "canonical_profile", "previous_hash", "event_hash", "actor", "created_at"];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) throw new Error("workspace consolidation profile table has an unsupported schema");
  const sql = objectSql(db, "table", "rm_consolidation_profile_events")?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const required of ["check(id > 0)", "check(event_type in ('proposed','approved','revoked'))", "check(length(profile_hash)=64", "check(length(event_hash)=64", "check(length(canonical_profile) between 2 and 524288)"]) {
    if (!sql.includes(required)) throw new Error("workspace consolidation profile table has unsupported constraints");
  }
}

function assertAuthTelemetrySchema(db: Database): void {
  const sql = objectSql(db, "table", "rm_workspace_auth_telemetry_events")
    ?.replace(/\s+/g, " ").toLowerCase() ?? "";
  if (!sql.includes("'two-factor-verify-backup-code'")) {
    throw new Error("workspace auth telemetry table lacks recovery-code evidence");
  }
  for (const [name, operation] of [
    ["rm_workspace_auth_telemetry_events_no_update", "UPDATE"],
    ["rm_workspace_auth_telemetry_events_no_delete", "DELETE"],
  ] as const) {
    const trigger = objectSql(db, "trigger", name);
    if (!trigger || !new RegExp(`BEFORE\\s+${operation}\\s+ON\\s+rm_workspace_auth_telemetry_events`, "i").test(trigger)) {
      throw new Error("workspace auth telemetry table is not append-only");
    }
  }
}

function assertStrictMigrationTable(db: Database): void {
  const columns = db.query("PRAGMA table_info(workspace_schema_migrations)").all() as Array<{
    name: string;
    notnull: number;
  }>;
  const required = [
    "id",
    "name",
    "checksum",
    "applied_at",
    "applied_by_version",
    "applied_by_commit",
  ];
  if (
    columns.length !== required.length ||
    required.some((name) => !columns.some((column) => column.name === name))
  ) {
    throw new Error("workspace control migration ledger has an unsupported schema");
  }
  for (const name of ["name", "checksum", "applied_at", "applied_by_version"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) {
      throw new Error("workspace control migration ledger must require checksummed provenance");
    }
  }
}

/** Validate the complete, checksummed append-only prefix known by this runtime. */
export function validateWorkspaceControlMigrationHistory(
  rows: readonly WorkspaceMigrationRow[],
): void {
  if (migrations.some((migration, index) => migration.id !== index + 1)) {
    throw new Error("workspace control migration catalog must be contiguous from version 1");
  }
  const newestApplied = rows.at(-1)?.id ?? 0;
  const newestSupported = migrations.at(-1)?.id ?? 0;
  if (newestApplied > newestSupported) {
    throw new Error(
      `workspace control schema version ${newestApplied} is newer than supported version ${newestSupported}; upgrade Rentemester before opening this workspace`,
    );
  }
  for (let index = 0; index < rows.length; index += 1) {
    const row = rows[index]!;
    const expected = migrations[index];
    if (!expected || row.id !== expected.id) {
      throw new Error("workspace control migration history is not a complete append-only prefix");
    }
    if (row.name !== expected.name) {
      throw new Error(`workspace control migration ${row.id} has unexpected name '${row.name}'`);
    }
    if (!row.checksum) {
      throw new Error("workspace control migration history contains a missing checksum");
    }
    if (row.checksum !== expected.checksum) {
      throw new Error(
        `workspace control migration ${row.id} checksum mismatch; the workspace control history may have been modified`,
      );
    }
  }
}

/**
 * Read-only compatibility check for an already-existing control database.
 * Opening/migrating it remains the responsibility of `openWorkspaceControlDb`.
 */
export function assertWorkspaceControlCompatibility(db: Database): void {
  const hasHistory = tableExists(db, "workspace_schema_migrations");
  if (!hasHistory) {
    const existingUserTable = db.query(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' LIMIT 1",
    ).get() as { name: string } | null;
    if (existingUserTable) {
      throw new Error(
        `workspace control database has table '${existingUserTable.name}' but no migration history; refusing to adopt unknown state`,
      );
    }
    return;
  }
  assertStrictMigrationTable(db);
  const rows = db.query(
    "SELECT id, name, checksum FROM workspace_schema_migrations ORDER BY id",
  ).all() as WorkspaceMigrationRow[];
  validateWorkspaceControlMigrationHistory(rows);
  if (rows.length === 0 && tableExists(db, "workspace_audit")) {
    throw new Error("workspace control audit table exists without its baseline migration record");
  }
}

/**
 * Checks the immutable audit and Better Auth primitives without changing DB
 * state.  Readiness uses this on a `query_only` connection.
 */
export function assertWorkspaceControlPrimitives(db: Database): void {
  if (
    !tableExists(db, "workspace_audit") ||
    !triggerExists(db, "workspace_schema_migrations_no_update") ||
    !triggerExists(db, "workspace_schema_migrations_no_delete") ||
    !triggerExists(db, "workspace_audit_no_update") ||
    !triggerExists(db, "workspace_audit_no_delete")
  ) {
    throw new Error(
      "workspace control audit primitives are incomplete; refusing to open workspace control database",
    );
  }

  // Better Auth owns the rows in these tables, but Rentemester owns the
  // reviewed, checksummed DDL which created them. Do not let Better Auth's
  // runtime migrator alter this workspace database.
  for (const table of ["user", "session", "account", "verification", "twoFactor", "rateLimit"]) {
    if (!tableExists(db, table)) {
      throw new Error(`workspace control Better Auth table '${table}' is missing`);
    }
  }

  for (const table of [
    "rm_workspace_user_access_events",
    "rm_company_membership_events",
    "rm_workspace_security_events",
    "rm_workspace_bootstrap_events",
    "rm_workspace_mfa_events",
    "rm_workspace_auth_telemetry_events",
    "rm_workspace_auth_state_events",
    "rm_workspace_document_access_events",
    "rm_workspace_authorization_events",
    "rm_workspace_invitation_events",
    "rm_intercompany_mapping_events",
    "rm_consolidation_elimination_events",
    "rm_consolidation_profile_events",
    "rm_party_events",
    "rm_party_alias_assertions",
    "rm_party_field_assertions",
    "rm_party_legacy_links",
    "rm_corporate_record_events",
    "rm_corporate_record_scope_assertions",
    "rm_company_knowledge_assertions",
    "rm_company_knowledge_events",
    "rm_ownership_source_snapshots",
    "rm_ownership_snapshot_events",
    "rm_ownership_facts",
    "rm_intercompany_dispositions",
    "rm_intercompany_disposition_events",
    "rm_intercompany_disposition_journal_links",
    "rm_intercompany_disposition_lifecycle_events",
  ]) {
    if (!tableExists(db, table)) {
      throw new Error(`workspace control access table '${table}' is missing`);
    }
    for (const operation of ["update", "delete"]) {
      if (!triggerExists(db, `${table}_no_${operation}`)) {
        throw new Error(`workspace control access table '${table}' is not append-only`);
      }
    }
  }
  assertAuthTelemetrySchema(db);
  assertInvitationSchema(db);
  if (!tableExists(db, "rm_group_manifest_events") ||
    !triggerExists(db, "rm_group_manifest_events_no_update") ||
    !triggerExists(db, "rm_group_manifest_events_no_delete")) {
    throw new Error("workspace group structure table is not append-only");
  }
  assertGroupManifestSchema(db);
  assertIntercompanyMappingSchema(db);
  assertConsolidationEliminationSchema(db);
  assertConsolidationProfileSchema(db);
  if (!triggerExists(db, "rm_better_auth_mfa_enrollment_revoke_sessions")) {
    throw new Error("workspace control MFA session-assurance trigger is missing");
  }
}

/**
 * Opens the workspace-local control database and applies its own immutable
 * migration catalog. This never opens, creates, or modifies a company ledger.
 */
export function openWorkspaceControlDb(workspaceRoot: string): Database {
  const paths = workspaceControlPaths(workspaceRoot);
  if (existsSync(paths.db)) {
    const preflight = new Database(paths.db, { readonly: true });
    try { assertLegacySourceActive(preflight); } finally { preflight.close(); }
  }
  mkdirSync(paths.directory, { recursive: true, mode: 0o700 });
  const db = new Database(paths.db);
  try {
    db.exec("PRAGMA foreign_keys = ON");
    assertWorkspaceControlCompatibility(db);
    const build = getBuildIdentity();
    db.transaction(() => {
      const applied = tableExists(db, "workspace_schema_migrations")
        ? db.query(
          "SELECT id, name, checksum FROM workspace_schema_migrations ORDER BY id",
        ).all() as WorkspaceMigrationRow[]
        : [];
      validateWorkspaceControlMigrationHistory(applied);
      for (const migration of migrations) {
        if (applied.some((row) => row.id === migration.id)) continue;
        const parsed = JSON.parse(migration.artifact.toString("utf8")) as {
          id: number;
          name: string;
          sql: string;
        };
        if (
          parsed.id !== migration.id ||
          parsed.name !== migration.name ||
          typeof parsed.sql !== "string"
        ) {
          throw new Error(`workspace control migration artifact ${migration.id} is malformed`);
        }
        db.exec(parsed.sql);
        db.query(
          `INSERT INTO workspace_schema_migrations
             (id, name, checksum, applied_by_version, applied_by_commit)
           VALUES (?, ?, ?, ?, ?)`,
        ).run(migration.id, migration.name, migration.checksum, build.version, build.gitCommit);
      }
    }).immediate();
    assertWorkspaceControlCompatibility(db);
    assertWorkspaceControlPrimitives(db);
    // Keep private workspace identity/control state inaccessible to other local users.
    chmodSync(paths.db, 0o600);
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

/**
 * Strict read-only control opening for status/report surfaces. Unlike the
 * writer this never creates directories, runs a migration, chmods a file, or
 * repairs state. A status route must fail closed on a missing/tampered DB.
 */
export function openWorkspaceControlReadOnlyDb(workspaceRoot: string): Database {
  const paths = workspaceControlPaths(workspaceRoot);
  if (!existsSync(paths.db)) throw new Error("workspace control database is unavailable");
  const db = openSqliteReadOnlySnapshot(paths.db);
  try {
    assertWorkspaceControlCompatibility(db);
    assertWorkspaceControlPrimitives(db);
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

export type WorkspaceAuditInput = {
  /** A stable action identifier, not a human-entered message or payload. */
  eventType: string;
  /** A stable resource class, such as a future `workspace_member` or `group`. */
  entityType: string;
  /** Opaque resource identifier only; never include credentials or document contents. */
  entityId?: string | number | null;
} & ResolveActorInput;

/**
 * Append a secret-free workspace control audit record. There intentionally is
 * no arbitrary message, JSON payload, before/after value, token or credential
 * parameter on this API; future features must store sensitive evidence outside
 * this general-purpose audit stream.
 */
export function insertWorkspaceAudit(db: Database, input: WorkspaceAuditInput): void {
  if (!input.eventType.trim() || !input.entityType.trim()) {
    throw new Error("workspace audit eventType and entityType are required");
  }
  const actor = resolveActor(input);
  db.query(
    "INSERT INTO workspace_audit (event_type, entity_type, entity_id, actor) VALUES (?, ?, ?, ?)",
  ).run(
    input.eventType.trim(),
    input.entityType.trim(),
    input.entityId == null ? null : String(input.entityId),
    actor.auditActor,
  );
}

export type WorkspaceDocumentAccessAuditInput = {
  actor: string;
  companySlug: string;
  resourceType: "document_file" | "issued_invoice_pdf";
  /** Null is valid only for an authorization denial where the object is not safely resolved. */
  resourceId: number | null;
  outcome: "served" | "denied";
  reasonCode: "authorized" | "authorization_denied";
  requestId?: string | null;
};

const SAFE_ACCESS_ACTOR = /^user:[A-Za-z0-9._-]{1,155}$/;
const SAFE_ACCESS_SLUG = /^[a-z0-9][a-z0-9-]{0,119}$/;
const SAFE_ACCESS_REQUEST_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;

/**
 * Append an allowlisted hosted document-access event.  This deliberately
 * accepts no filename, path, MIME, request URL/body, byte count, exception or
 * arbitrary metadata, so the evidence store itself cannot become a leak.
 */
export function insertWorkspaceDocumentAccessAudit(
  db: Database,
  input: WorkspaceDocumentAccessAuditInput,
): void {
  if (!SAFE_ACCESS_ACTOR.test(input.actor) || !SAFE_ACCESS_SLUG.test(input.companySlug)) {
    throw new Error("invalid document access audit identity");
  }
  if (!Number.isInteger(input.resourceId) && input.resourceId !== null) {
    throw new Error("invalid document access audit resource");
  }
  if (input.resourceId !== null && input.resourceId <= 0) {
    throw new Error("invalid document access audit resource");
  }
  if ((input.outcome === "served") !== (input.reasonCode === "authorized")) {
    throw new Error("invalid document access audit outcome");
  }
  const requestId = input.requestId?.trim() || null;
  if (requestId !== null && !SAFE_ACCESS_REQUEST_ID.test(requestId)) {
    throw new Error("invalid document access audit request id");
  }
  db.query(
    `INSERT INTO rm_workspace_document_access_events
       (actor, company_slug, resource_type, resource_id, outcome, reason_code, request_id)
     VALUES (?, ?, ?, ?, ?, ?, ?)`,
  ).run(
    input.actor,
    input.companySlug,
    input.resourceType,
    input.resourceId,
    input.outcome,
    input.reasonCode,
    requestId,
  );
}

export type WorkspaceAuthorizationAuditInput = {
  actor: string;
  method: string;
  routeTemplate: string;
  permission: string;
  companySlug?: string | null;
  requestId?: string | null;
};

const SAFE_AUTHORIZATION_ROUTE = /^\/api\/[A-Za-z0-9_/:.-]{1,235}$/;
const SAFE_AUTHORIZATION_PERMISSION = /^[a-z0-9][a-z0-9._-]{2,79}$/;

/** Records only a route template and authorization decision—never raw URL, query, body or object id. */
export function insertWorkspaceAuthorizationAudit(
  db: Database,
  input: WorkspaceAuthorizationAuditInput,
): void {
  const method = input.method.trim().toUpperCase();
  const companySlug = input.companySlug?.trim() || null;
  const requestId = input.requestId?.trim() || null;
  if (!SAFE_ACCESS_ACTOR.test(input.actor) || !["GET", "POST", "PATCH", "PUT", "DELETE"].includes(method)) {
    throw new Error("invalid authorization audit identity");
  }
  if (!SAFE_AUTHORIZATION_ROUTE.test(input.routeTemplate) || !SAFE_AUTHORIZATION_PERMISSION.test(input.permission)) {
    throw new Error("invalid authorization audit route");
  }
  if (companySlug !== null && !SAFE_ACCESS_SLUG.test(companySlug)) {
    throw new Error("invalid authorization audit company");
  }
  if (requestId !== null && !SAFE_ACCESS_REQUEST_ID.test(requestId)) {
    throw new Error("invalid authorization audit request id");
  }
  db.query(`INSERT INTO rm_workspace_authorization_events
    (actor, method, route_template, permission, company_slug, outcome, reason_code, request_id)
    VALUES (?, ?, ?, ?, ?, 'denied', 'authorization_denied', ?)`).run(
    input.actor,
    method,
    input.routeTemplate,
    input.permission,
    companySlug,
    requestId,
  );
}

export function readWorkspaceControlMigrations(db: Database): WorkspaceMigrationRow[] {
  return db.query(
    "SELECT id, name, checksum FROM workspace_schema_migrations ORDER BY id",
  ).all() as WorkspaceMigrationRow[];
}
