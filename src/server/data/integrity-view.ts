// Integritet & backup-panel (#333).
//
// Genbruger `verifyAuditChain` (hash-chain + foreign-key + balance check)
// fra ledger.ts og `getBackupComplianceStatus` + `listBackupDestinations`
// fra system-backups / backup-governance. Ingen genimplementering.
//
// Verifikationen er idempotent — `verifyAuditChain` er read-only og kan
// kaldes så ofte cockpittet ønsker uden side-effekter.

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import { verifyAuditChain } from "../../core/ledger";
import { getBackupComplianceStatus } from "../../core/system-backups";
import { listBackupDestinations } from "../../core/backup-governance";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type IntegrityCompany = {
  name: string;
  cvr: string | null;
  country: string;
  currency: string;
};

export type AuditChainStatus = {
  ok: boolean;
  entries: number;
  errors: string[];
};

export type BackupDestinationSummary = {
  id: string;
  label: string;
  kind: string;
  location: string;
  inEeaOrEu: boolean;
  country: string | null;
  meetsRecognisedStandards: boolean | null;
  nonRelatedParty: boolean;
  lastPlacementAt: string | null;
};

export type CompanyIntegrityView = {
  slug: string;
  company: IntegrityCompany;
  auditChain: AuditChainStatus;
  backup: {
    ok: boolean;
    latestBackupAt: string | null;
    latestBackupId: string | null;
    backupDue: boolean;
    hasActivitySinceBackup: boolean;
    daysSinceLatestBackup: number | null;
    backupsFound: number;
    requiredBy: string | null;
    checkedAt: string;
  };
  destinations: BackupDestinationSummary[];
  legalCitation: {
    sourceId: string;
    note: string;
  };
};

export function buildCompanyIntegrity(
  workspaceRoot: string,
  slug: string,
): CompanyIntegrityView {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }
  const db = openDb(dbPath);
  try {
    migrate(db);
    const companySettings = getCompanySettings(db);
    const audit = verifyAuditChain(db);
    const backup = getBackupComplianceStatus(db, companyRoot);
    const destinations = listBackupDestinations(companyRoot).map((d) => ({
      id: d.id,
      label: d.label,
      kind: d.kind,
      location: d.location,
      inEeaOrEu: d.regionAttestation.inEeaOrEu,
      country: d.regionAttestation.country,
      meetsRecognisedStandards:
        d.itSecurityAttestation?.meetsRecognisedStandards ?? null,
      nonRelatedParty: d.nonRelatedParty,
      lastPlacementAt:
        d.placements.length > 0
          ? d.placements
              .map((p) => p.placedAt)
              .sort()
              .slice(-1)[0]
          : null,
    }));
    return {
      slug,
      company: {
        name: companySettings.name,
        cvr: companySettings.cvr,
        country: companySettings.country,
        currency: companySettings.currency,
      },
      auditChain: {
        ok: audit.ok,
        entries: audit.entries,
        errors: audit.errors,
      },
      backup: {
        ok: backup.ok,
        latestBackupAt: backup.latestBackupAt,
        latestBackupId: backup.latestBackupId,
        backupDue: backup.backupDue,
        hasActivitySinceBackup: backup.hasActivitySinceBackup,
        daysSinceLatestBackup: backup.daysSinceLatestBackup,
        backupsFound: backup.backupsFound,
        requiredBy: backup.requiredBy,
        checkedAt: backup.checkedAt,
      },
      destinations,
      legalCitation: {
        sourceId: "DK-BOGFORINGSLOVEN-2022-700",
        note:
          "Bogføringsloven § 14 — bogføringsmaterialet skal opbevares forsvarligt, " +
          "og bogføringssystemet skal sikre, at posteringer ikke kan ændres efter de er bogført.",
      },
    };
  } finally {
    db.close();
  }
}
