import type {
  CvrSystemStatusResponse,
  HealthResponse,
  PortfolioResponse,
  RulesResponse,
} from "../types";
import { request } from "./_shared";

export const systemApi = {
  health: () => request<HealthResponse>("/api/health"),

  portfolio: (asOf?: string) =>
    request<PortfolioResponse>(
      `/api/portfolio${asOf ? `?asOf=${encodeURIComponent(asOf)}` : ""}`,
    ).then((r) => r.portfolio),

  /**
   * #347 — Lovgrundlag-viewer. Henter alle aktive regler i `rules/dk/*.yaml`
   * sammen med deres SHA-256-citationer og legal-sources, så cockpittet kan
   * vise SMB-ejeren hvilke regler der styrer bogføringen og hvor de er hentet
   * fra.
   */
  rules: () => request<RulesResponse>("/api/rules"),

  /**
   * #402 — whether the server has CVR_USERNAME/CVR_PASSWORD set, so the
   * cockpit can disable "Hent fra CVR" with a friendly note instead of
   * letting the owner trigger a silent failure.
   */
  cvrStatus: () =>
    request<CvrSystemStatusResponse>("/api/system/cvr-status").then(
      (r) => r.cvrStatus,
    ),
};
