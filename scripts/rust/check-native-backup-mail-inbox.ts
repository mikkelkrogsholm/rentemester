import type { Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";

type Options = { company?: string; sourceId?: string; id?: number; success?: boolean };
type Invoke = (kind: "cli" | "mcp", command: string, body?: unknown, options?: Options) => any;
/** Extends the same cold normal-MFA backup proof; never connects to an IMAP/SMTP provider. */
export function createBackupMailInboxProof(page: Page, base: string, invoke: Invoke) {
  const anchor = "synthetic", target = "synthetic-backup-inbox-target", sourceId = "synthetic-backup-inbox-source-113";
  const original = Buffer.from("Synthetic backup locked inbox source 113\nNo personal or production data.\n");
  const digest = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
  const alias = { alias: "synthetic-backup-lock-mail-113" }, destination = { companySlug: target, confirm: true };
  const need = (value: unknown, label: string) => { if (!value) throw Error(label); };
  let baseline: any, stable: any, documentId = 0, prepared = false, completed = false;
  async function api(company: string, resource: string, body?: unknown, method = "POST") {
    const response = await page.request.fetch(`${base}/api/companies/${company}/${resource}`, body === undefined ? {} : { method, headers: { origin: base }, data: body });
    need(response.ok(), `Backup inbox ${company}/${resource} HTTP${response.status()}`); return response.json();
  }
  async function source() { return api(anchor, `workspace-inbox/${sourceId}`); }
  async function targetState() {
    return { journal: (await api(target, "journal")).journal, bank: (await api(target, "bank")).bank, documents: (await api(target, "documents")).documents };
  }
  async function snapshot() {
    return { source: await source(), mail: await api(anchor, "bilagsmail"), target: await targetState(),
      anchorJournal: (await api(anchor, "journal")).journal, anchorDocuments: (await api(anchor, "documents")).documents };
  }
  async function unchanged() { need(isDeepStrictEqual(await snapshot(), baseline), "Refused inbox/mail action changed source, mail state or either company financial/document state"); }
  async function refusal(resource: string, command: string, body: unknown, method = "POST", options: Options = {}) {
    const response = await page.request.fetch(`${base}/api/companies/${anchor}/${resource}`, { method, headers: { origin: base }, data: body });
    const value = await response.json(); need(!response.ok() && value.ok === false && value.code === "backup_overdue", `Current ${resource} must reach backup_overdue`);
    await unchanged(); const clients: Record<string, unknown> = {};
    for (const kind of ["cli", "mcp"] as const) { clients[kind] = invoke(kind, command, body, { ...options, success: false }); await unchanged(); }
    return { http: { status: response.status(), code: value.code }, clients };
  }
  async function parity(expected: any) {
    for (const kind of ["cli", "mcp"] as const) {
      need(isDeepStrictEqual(invoke(kind, "workspace_inbox_source", undefined, { sourceId }), expected.source), `${kind} exact inbox source/history parity`);
      need(isDeepStrictEqual(invoke(kind, "bilagsmail"), expected.mail), `${kind} exact alias/status parity`);
      need(isDeepStrictEqual(invoke(kind, "documents", undefined, { company: target }).documents, expected.target.documents), `${kind} target document parity`);
      const bytes = invoke(kind, "document_original", undefined, { company: target, id: documentId }).original;
      need(bytes.contentType === "text/plain" && Buffer.from(bytes.bytes).equals(original), `${kind} exact handed-off original bytes`);
    }
  }
  async function exactOriginal() {
    const response = await page.request.get(`${base}/api/companies/${target}/documents/${documentId}/file`);
    need(response.ok() && (await response.body()).equals(original), "Exact HTTP handed-off original bytes");
  }
  return {
    async beforeALock(historical: string) {
      need(!prepared, "Prepare synthetic target/source once");
      const created = await page.request.post(base + "/api/companies", { headers: { origin: base }, data: { name: "Synthetic backup inbox target", slug: target } });
      need(created.status() === 201, "Normal owner creates independent target company");
      await api(target, "accounts", { accountNo: "1713", name: "Synthetic inbox target bank", type: "asset", normalBalance: "debit" });
      const account = (await api(target, "bank-accounts", { name: "Synthetic inbox target bank", slug: "synthetic-inbox-target-bank", currency: "DKK", ledgerAccountNo: "1713" })).bankAccount;
      const bytes = Buffer.from(`date,text,amount,currency,reference\n${historical},Synthetic inbox target historical activity,13.00,DKK,SYNTHETIC-BACKUP-INBOX-113\n`);
      const imported = (await api(target, "bank/import", { csvBase64: bytes.toString("base64"), account: "synthetic-inbox-target-bank", confirm: true })).import;
      need(imported.imported === 1 && imported.bankAccountId === account.id && imported.sourceFileHash === digest(bytes), "Target has one exact historical bank activity without a journal");
      await api(target, "backup-lock", { confirm: true, enforced: true, graceDays: 0 });
      const targetStatus = await api(target, "backup-status"), anchorStatus = await api(anchor, "backup-status");
      need(targetStatus.lock.locked && !anchorStatus.lock.locked, "Assignment proof needs unlocked anchor and independently locked target");
      const input = { confirm: true, sourceId, idempotencyKey: "synthetic-backup-inbox-source-key-113", bytesBase64: original.toString("base64"), filename: "synthetic-backup-inbox-source-113.txt", mimeType: "text/plain", transport: "upload", receivedAt: historical, metadata: { documentType: "cash_register_receipt", currency: "DKK" }, candidates: [{ companySlug: target, evidenceKind: "account", evidence: "synthetic-inbox-target-bank" }] };
      const ingested = invoke("cli", "ingest_workspace_inbox", input);
      need(ingested.source?.sourceId === sourceId, "Source-only candidate ingestion succeeds while candidate company is locked");
      baseline = await snapshot();
      need(baseline.target.documents.documents.length === 0 && baseline.target.journal.entries.length === 0 && baseline.target.bank.transactions.length === 1, "Target starts unposted with no document handoff");
      need(baseline.target.journal.ledgerUuid !== baseline.anchorJournal.ledgerUuid, "Independent company ledger identities required");
      const assignmentRefusal = await refusal(`workspace-inbox/${sourceId}/assign`, "assign_workspace_inbox", destination, "POST", { sourceId });
      prepared = true;
      return { target, sourceId, targetLedgerUuid: baseline.target.journal.ledgerUuid, targetBankSourceSha256: digest(bytes), originalSha256: digest(original), anchorUnlocked: true, targetLocked: true, sourceCandidateDidNotGate: true, assignmentRefusal };
    },
    async locked() {
      need(prepared && !completed, "Prepared locked inbox target required"); await unchanged();
      const current = await api(anchor, "backup-status"); need(current.lock.locked, "Alias proof uses current locked anchor");
      const aliasRefusal = await refusal("bilagsmail/alias", "set_bilagsmail_alias", alias, "PATCH");
      return { aliasRefusal, humanAliasGateQualified: true, imapConfigurationQualified: false, realImapQualified: false, smtpQualified: false };
    },
    async unlocked() {
      need(prepared && !completed, "Complete the same target handoff once");
      const status = await api(target, "backup-status"); need(status.lock.enforced && !status.lock.locked && status.backup.ok, "Current whole-workspace backup also unlocks target B");
      invoke("mcp", "assign_workspace_inbox", destination, { sourceId });
      const assigned = await source(); need(assigned.source.assignments.some((item: any) => item.companySlug === target && item.state === "approved"), "Explicit target approval precedes completion");
      const result = invoke("cli", "complete_workspace_inbox", destination, { sourceId });
      const handoff = result.source.assignments.find((item: any) => item.companySlug === target && item.state === "completed");
      need(handoff?.documentId > 0 && result.source.history.filter((item: any) => item.event_type === "handoff_completed").length === 1, "Exactly one recorded handoff");
      documentId = handoff.documentId;
      for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(invoke(kind, "complete_workspace_inbox", destination, { sourceId }), result), "Explicit completion replay returns same document and canonical source history");
      await api(anchor, "bilagsmail/alias", alias, "PATCH");
      stable = { source: await source(), mail: await api(anchor, "bilagsmail"), target: await targetState() };
      need(stable.target.documents.documents.length === 1 && stable.target.documents.documents[0].id === documentId
        && stable.target.documents.documents[0].source === `workspace-inbox:${sourceId}`
        && stable.target.journal.entries.length === 0 && stable.target.bank.transactions.length === 1
        && stable.mail.bilagsmail.mailAlias === alias.alias, "One target document, zero target postings and one explicit alias update");
      need((await api(anchor, "documents")).documents.documents.length === 0, "Source anchor never receives target document");
      await exactOriginal(); await parity(stable); completed = true;
      return { target, sourceId, documentId, originalSha256: digest(original), currentWholeWorkspaceBackupUnlocksTarget: true,
        completionReplaySameDocument: true, exactClientOriginalParity: true, targetCanonical: stable.target, source: stable.source,
        twoCompanyInboxTargetGateQualified: true, humanAliasGateQualified: true, imapConfigurationQualified: false, smtpQualified: false };
    },
    async reopen() {
      need(completed, "Completed exact inbox flow required before fresh-MFA restart");
      const current = { source: await source(), mail: await api(anchor, "bilagsmail"), target: await targetState() };
      need(isDeepStrictEqual(current, stable), "Exact inbox source/history/alias/target documents/bank/journal persist across restart");
      for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(invoke(kind, "complete_workspace_inbox", destination, { sourceId }), stable.source), "Post-restart explicit completion reconciliation creates no duplicate");
      await exactOriginal(); await parity(stable);
      need(isDeepStrictEqual(await targetState(), stable.target), "Restart replay has no duplicate document or journal effects");
      return { target, sourceId, documentId, normalMfaRestartQualified: true, exactSourceAliasAndOriginalPersist: true, targetTerminalCounts: { documents: 1, bankTransactions: 1, journalEntries: 0 } };
    },
  };
}
