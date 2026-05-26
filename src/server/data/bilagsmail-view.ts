// Bilagsmail view (#348/#350/#351).
//
// Aggregerer pr.-virksomhed:
//  - IMAP-config status (uden at returnere passwordet)
//  - Mail-alias
//  - De seneste indlæste dokumenter fra mail-drop-kilden (#351 inbox-view)

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import {
  getCompanyMailAlias,
  loadBilagsmailImapConfig,
} from "../../core/bilagsmail";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type BilagsmailInboxRow = {
  id: number;
  documentNo: string | null;
  source: string;
  uploadDatetime: string | null;
  senderName: string | null;
  invoiceDate: string | null;
  amountIncVat: number | null;
  retainUntil: string | null;
};

export type CompanyBilagsmailView = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  /** True når en gemt config findes — passwordet selv lækkes aldrig over API'et. */
  imapConfigured: boolean;
  /** Sikre detaljer fra IMAP-konfigurationen (host/port/username/mailbox/secure). */
  imapStatus: {
    host: string;
    port: number;
    secure: boolean;
    username: string;
    mailbox: string;
  } | null;
  /** Mail-alias (localpart) — kan være null hvis ikke konfigureret. */
  mailAlias: string | null;
  /** Inbox: senest indlæste mail-drop-dokumenter. */
  inbox: BilagsmailInboxRow[];
};

export function buildCompanyBilagsmail(
  workspaceRoot: string,
  slug: string,
): CompanyBilagsmailView {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }
  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);
    const imap = loadBilagsmailImapConfig(companyRoot);
    const mailAlias = getCompanyMailAlias(db);

    // Senest indlæste mail-drop-dokumenter — vi filtrerer på source-præfikset
    // "mail" så både `mail`, `imap` og varianter (`mail-drop`) fanges.
    const inboxRows = db
      .query(
        `SELECT id, document_no, source, upload_datetime, sender_name,
                invoice_date, amount_inc_vat, retain_until
           FROM documents
          WHERE source LIKE 'mail%' OR source = 'imap'
          ORDER BY id DESC
          LIMIT 50`,
      )
      .all() as Array<{
      id: number;
      document_no: string | null;
      source: string;
      upload_datetime: string | null;
      sender_name: string | null;
      invoice_date: string | null;
      amount_inc_vat: number | null;
      retain_until: string | null;
    }>;

    const inbox: BilagsmailInboxRow[] = inboxRows.map((r) => ({
      id: r.id,
      documentNo: r.document_no,
      source: r.source,
      uploadDatetime: r.upload_datetime,
      senderName: r.sender_name,
      invoiceDate: r.invoice_date,
      amountIncVat: r.amount_inc_vat,
      retainUntil: r.retain_until,
    }));

    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
      },
      imapConfigured: imap !== null,
      imapStatus: imap
        ? {
            host: imap.host,
            port: imap.port,
            secure: imap.secure ?? true,
            username: imap.username,
            mailbox: imap.mailbox ?? "INBOX",
          }
        : null,
      mailAlias,
      inbox,
    };
  } finally {
    db.close();
  }
}
