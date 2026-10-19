import { ApiError, requestBlob } from "./_shared";

/** Workspace export; neither a company selector nor a server filesystem path. */
export async function downloadNativeBackup() {
  const response = await requestBlob("/api/workspace/backup", {
    method: "POST",
    body: JSON.stringify({ confirm: true }),
  });
  if (response.headers.get("content-type")?.split(";")[0]?.trim() !== "application/vnd.rentemester.native-backup+json" || response.blob.size === 0) {
    throw new ApiError("internal", "Serverens svar kunne ikke bekræftes som en backupfil.", 200);
  }
  return response.blob;
}
