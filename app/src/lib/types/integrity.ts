// #333 — Integritet & backup-panel.

export type AuditChainStatus = {
  ok: boolean | null;
  entries: number | null;
  available?: boolean;
  scope?: string;
  scopeNote?: string;
  unavailableReason?: string;
  errors: string[];
};

export type BackupStatusSummary = {
  ok: boolean | null;
  latestBackupAt: string | null;
  latestBackupId: string | null;
  backupDue: boolean | null;
  hasActivitySinceBackup: boolean | null;
  daysSinceLatestBackup: number | null;
  backupsFound: number | null;
  requiredBy: string | null;
  checkedAt: string | null;
  available?: boolean;
  unavailableReason?: string;
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
  destinationsAvailable?: boolean;
  destinationsUnavailableReason?: string;
  legalCitation: { sourceId: string; note: string };
};

export type IntegrityResponse = {
  ok: true;
  integrity: CompanyIntegrity;
};
