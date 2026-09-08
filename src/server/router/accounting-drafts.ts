import { openCurrentLedgerReadOnly } from "../../core/ledger-inspection";
import { getAccountingDraft, listAccountingDrafts } from "../../core/accounting-drafts";
import { requireCompanyDbPath } from "../data/shared";
import type { ServerConfig } from "../config";
import { okResponse } from "./_shared";

export function handleCompanyAccountingDrafts(config: ServerConfig, slug: string): Response {
  const db = openCurrentLedgerReadOnly(requireCompanyDbPath(config.workspaceRoot, slug));
  try {
    return okResponse({ accountingDrafts: listAccountingDrafts(db) });
  } finally {
    db.close();
  }
}

export function handleCompanyAccountingDraft(config: ServerConfig, slug: string, draftId: string): Response {
  const db = openCurrentLedgerReadOnly(requireCompanyDbPath(config.workspaceRoot, slug));
  try {
    return okResponse({ accountingDraft: getAccountingDraft(db, draftId) });
  } finally {
    db.close();
  }
}
