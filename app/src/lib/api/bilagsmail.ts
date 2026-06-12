import type { BilagsmailResponse } from "../types";
import { request } from "./_shared";

export const bilagsmailApi = {
  /**
   * #348/#350/#351 — Bilagsmail read state (config, alias, inbox).
   */
  bilagsmail: (slug: string) =>
    request<BilagsmailResponse>(
      `/api/companies/${encodeURIComponent(slug)}/bilagsmail`,
    ).then((r) => r.bilagsmail),

  /** #348 — Save the per-company IMAP config to disk. */
  saveBilagsmailImapConfig: (
    slug: string,
    body: {
      host: string;
      port: number;
      username: string;
      password: string;
      secure?: boolean;
      mailbox?: string;
    },
  ) =>
    request<{ ok: true }>(
      `/api/companies/${encodeURIComponent(slug)}/bilagsmail/imap-config`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ),

  /** #348 — Delete the stored IMAP config. */
  deleteBilagsmailImapConfig: (slug: string) =>
    request<{ ok: true }>(
      `/api/companies/${encodeURIComponent(slug)}/bilagsmail/imap-config`,
      { method: "DELETE" },
    ),

  /** #350 — Set or clear the per-company mail alias. */
  setBilagsmailAlias: (slug: string, alias: string | null) =>
    request<{ ok: true }>(
      `/api/companies/${encodeURIComponent(slug)}/bilagsmail/alias`,
      {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ alias }),
      },
    ),
};
