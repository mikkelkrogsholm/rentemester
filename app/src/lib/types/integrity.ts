// #333 — Integritet & backup-panel.

export type AuditChainStatus = {
  ok: boolean;
  entries: number;
  errors: string[];
};

export type BackupStatusSummary = {
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

export type CompanyIntegrity = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  auditChain: AuditChainStatus;
  backup: BackupStatusSummary;
  destinations: BackupDestinationSummary[];
  legalCitation: { sourceId: string; note: string };
};

export type IntegrityResponse = {
  ok: true;
  integrity: CompanyIntegrity;
};
