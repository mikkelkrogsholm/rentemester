import { parseFilenameFromContentDisposition, requestBlob } from "./_shared";

export const accountantApi = {
  /** Existing bounded custom v3 profile; audited package, no official validation/submission. */
  saftExport: async (slug: string, input: {periodStart:string;periodEnd:string}) => {
    const res = await requestBlob(`/api/companies/${encodeURIComponent(slug)}/saft-export`, {
      method: "POST", body: JSON.stringify({...input, confirm:true}),
    });
    const count = (name: string) => {
      const value = res.headers.get(name);
      if (value === null || !/^\d+$/.test(value)) throw new Error("Eksportens optælling mangler i serversvaret.");
      return Number(value);
    };
    return {blob:res.blob, filename:parseFilenameFromContentDisposition(res.headers.get("content-disposition")) ?? `saft-eksport-${slug}-${input.periodStart}-${input.periodEnd}.tar`,
      journalEntryCount:count("x-rentemester-journal-entries"),
      documentCount:count("x-rentemester-documents"), profile:"bounded-saft-v3" as const};
  },
  /**
   * Generates the accountant-handoff package and returns it as a downloadable
   * .tar blob — the cockpit's "share with revisor" action. The server packs
   * the same files the CLI's `system export-accountant` produces; the browser
   * triggers a download from the returned blob.
   */
  accountantExport: async (
    slug: string,
    input: { periodStart: string; periodEnd: string },
  ): Promise<AccountantExportResult> => {
    const res = await requestBlob(
      `/api/companies/${encodeURIComponent(slug)}/accountant-export`,
      {
        method: "POST",
        body: JSON.stringify({ ...input, confirm: true }),
      },
    );
    const filename =
      parseFilenameFromContentDisposition(res.headers.get("content-disposition")) ??
      `revisor-eksport-${slug}-${input.periodStart}-${input.periodEnd}.tar`;
    return {
      blob: res.blob,
      filename,
      journalEntryCount: Number(
        res.headers.get("x-rentemester-journal-entries") ?? 0,
      ),
      documentCount: Number(res.headers.get("x-rentemester-documents") ?? 0),
      bankTransactionCount: Number(
        res.headers.get("x-rentemester-bank-transactions") ?? 0,
      ),
    };
  },
};

/** The accountant-export result the server echoes back, plus the tar blob. */
export type AccountantExportResult = {
  /** The .tar archive as a Blob — the browser triggers a download from this. */
  blob: Blob;
  /** Suggested download filename derived from the response. */
  filename: string;
  /** Number of journal entries included — surfaced in the UI receipt. */
  journalEntryCount: number;
  /** Number of supporting documents included. */
  documentCount: number;
  /** Number of bank transactions included. */
  bankTransactionCount: number;
};
