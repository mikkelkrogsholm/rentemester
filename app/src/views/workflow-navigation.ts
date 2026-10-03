/** List context is an internal, company-scoped URL, never an arbitrary return URL. */
export type DailyList = "bilag" | "fakturaer";
const LIST_KEYS: Record<DailyList, readonly string[]> = {
  bilag: ["year", "q", "from", "to", "status", "type", "party", "documentId", "sort", "dir", "page", "pageSize"],
  fakturaer: ["year", "q", "status", "from", "to", "sort", "dir", "page", "pageSize"],
};

export function listReturnTo(slug: string, list: DailyList, raw?: string | null, year?: string | null): string {
  const pathname = `/companies/${encodeURIComponent(slug)}/${list}`;
  const out = new URLSearchParams();
  if (raw) {
    try {
      const candidate = new URL(raw, "https://rentemester.invalid");
      if (candidate.origin === "https://rentemester.invalid" && candidate.pathname === pathname) {
        for (const key of LIST_KEYS[list]) {
          const value = candidate.searchParams.get(key);
          if (value !== null) out.set(key, value);
        }
      }
    } catch { /* An invalid return target falls back to the company list. */ }
  }
  if (year && !out.has("year")) out.set("year", year);
  return `${pathname}${out.size ? `?${out}` : ""}`;
}

export function workflowTo(slug: string, list: DailyList, suffix: string, params: URLSearchParams, year?: string): string {
  const returnTo = listReturnTo(slug, list, params.get("returnTo") ?? `/companies/${encodeURIComponent(slug)}/${list}?${params}`, year);
  const query = new URLSearchParams({ returnTo });
  const selectedYear = year ?? params.get("year");
  if (selectedYear) query.set("year", selectedYear);
  return `/companies/${encodeURIComponent(slug)}/${list}/${suffix}?${query}`;
}

export function listPagination(params: URLSearchParams, total: number) {
  const requestedSize = Number(params.get("pageSize"));
  const pageSize = requestedSize === 25 || requestedSize === 100 ? requestedSize : 50;
  const requestedPage = Number(params.get("page"));
  const page = Math.min(Math.max(1, Number.isSafeInteger(requestedPage) ? requestedPage : 1), Math.max(1, Math.ceil(total / pageSize)));
  return { page, pageSize, offset: (page - 1) * pageSize };
}
