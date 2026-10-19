import { ApiError, request, requestBlob, type ReadRequestOptions } from "./_shared";
import type { WorkspaceMode } from "../workspace-write-mode";
export type TransferChallenge = { version: 1; transferId: string; sourceInstanceId: string; workspaceId: string; archiveSha256: string; destinationInstanceId: string; destination: string; companies: Array<{ slug: string; ledgerUuid: string }> };
export type Transfer = { mode: WorkspaceMode; stateHash?: string; transferId?: string | null; destination?: string | null; challenge?: TransferChallenge | null; release?: string | null; offeredArchiveHashes?: string[]; pendingEffects?: Array<{ companySlug: string; messageId: string; state: "unknown" }>; history?: unknown[] };
export type TransferReview = { expectedStateHash: string; confirm: true };
export type TransferFreeze = TransferReview & { destination: string; reason: string };
export type TransferChange = TransferReview & { reason: string };
export type TransferRelease = TransferChange & { challenge: TransferChallenge };
export type TransferActivate = TransferChange & { release: string };
export type RestoredReceipt = { receiptId: number; operation: "journal_post" | "journal_reverse"; keyHash: string; payloadHash: string; ledgerUuid: string; sourcePrincipal: { kind: string; subject: string }; createdAt: string; expiresAt: string; archiveSha256: string; restoreBoundaryId: number | string; resultAvailable: boolean; result: unknown; stateHash: string; mapping: null | { targetUserId: string; actor: string; at: string; reason: string; archiveSha256: string; restoreBoundaryId: number | string } };
function checked(value: { ok: true; transfer: Transfer }) {
  if (value?.ok !== true || !["active", "frozen", "moved", "staged"].includes(value.transfer?.mode)) throw new ApiError("internal", "Workspace-tilstanden kunne ikke bekræftes.", 200);
  return value.transfer;
}
const base = "/api/workspace/transfer";
export const workspaceTransferApi = {
  status: (options?: ReadRequestOptions) => request<{ ok: true; transfer: Transfer }>(base, options).then(checked),
  freeze: (body: TransferFreeze) => request<{ ok: true; transfer: Transfer }>(`${base}/freeze`, { method: "POST", body: JSON.stringify(body) }).then(checked),
  cancel: (body: TransferChange) => request<{ ok: true; transfer: Transfer }>(`${base}/cancel`, { method: "POST", body: JSON.stringify(body) }).then(checked),
  release: (body: TransferRelease) => request<{ ok: true; transfer: Transfer }>(`${base}/release`, { method: "POST", body: JSON.stringify(body) }).then(checked),
  activate: (body: TransferActivate) => request<{ ok: true; transfer: Transfer }>(`${base}/activate`, { method: "POST", body: JSON.stringify(body) }).then(checked),
  export: async (body: TransferReview) => {
    const result = await requestBlob(`${base}/export`, { method: "POST", body: JSON.stringify(body) });
    if (result.headers.get("content-type")?.split(";")[0]?.trim() !== "application/vnd.rentemester.native-backup+json" || !result.blob.size) throw new ApiError("internal", "Flyttearkivet kunne ikke bekræftes.", 200);
    return result.blob;
  },
  receipts: (slug: string, options?: ReadRequestOptions) => request<{ ok: true; restoredReceipts: { rows: RestoredReceipt[] } }>(`/api/companies/${encodeURIComponent(slug)}/restore/receipts`, options).then(r => r.restoredReceipts),
  mapReceipt: (slug: string, body: { receiptId: number; expectedStateHash: string; reason: string; confirm: true }) => request<{ ok: true; restoredReceipts: { rows: RestoredReceipt[] } }>(`/api/companies/${encodeURIComponent(slug)}/restore/receipts/reconcile`, { method: "POST", body: JSON.stringify(body) }).then(r => r.restoredReceipts),
};
export function fullTransfer(t: Transfer): boolean {
  return /^[a-f0-9]{64}$/.test(t.stateHash ?? "") && Array.isArray(t.offeredArchiveHashes) && Array.isArray(t.pendingEffects) && Array.isArray(t.history);
}
