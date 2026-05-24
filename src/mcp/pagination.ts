/**
 * Shared pagination kontrakt for alle MCP `*_list`-tools (#381).
 *
 * Før #381 returnerede flere list-tools hele tabellen uden cap. Det kan
 * sprænge en agents context-vindue og koste den hele arbejdsgangen midt
 * i et tool-kald. Agenten kunne heller ikke vide om svaret var komplet
 * eller trunkeret.
 *
 * Denne modul leverer ÉN fælles kontrakt:
 *
 *   - `paginationFields` — `limit` og `offset` som valgfri input-felter
 *     med ens beskrivelser på tværs af alle list-tools.
 *   - `paginationDescriptionSuffix` — den prose-bid hvert list-tools
 *     `description` skal slutte med, så agenten kan læse pagination-
 *     kontrakten direkte fra `tools/list` uden at slå op andetsteds.
 *   - `applyPagination(rows, args)` — slicer en array efter limit/offset
 *     og returnerer paginations-metadata.
 *
 * Default-limit er `DEFAULT_PAGE_LIMIT = 500`. Hard-cap er
 * `MAX_PAGE_LIMIT = 5000` — over dette afvises inputtet (Zod-niveau).
 */

import { z } from "zod";

export const DEFAULT_PAGE_LIMIT = 500;
export const MAX_PAGE_LIMIT = 5000;

/**
 * Zod-felter til inputSchema. Spread'es ind i hvert list-tools
 * `inputSchema` så `limit`/`offset` har samme form og samme beskrivelse
 * overalt.
 */
export const paginationFields = {
  limit: z
    .number()
    .int()
    .min(1)
    .max(MAX_PAGE_LIMIT)
    .optional()
    .describe(
      `Max antal rækker i dette svar. Default ${DEFAULT_PAGE_LIMIT}, hard-cap ${MAX_PAGE_LIMIT}. ` +
        "Brug sammen med `offset` til at hente næste side.",
    ),
  offset: z
    .number()
    .int()
    .min(0)
    .optional()
    .describe(
      "Antal rækker der skal springes over før denne side. Default 0. " +
        "Næste side: send `offset = forrige offset + count` (eller læs `nextOffset` fra svaret).",
    ),
} as const;

/** Tekst der appendes til hvert list-tools `description`. */
export const paginationDescriptionSuffix =
  ` Paginering: returnerer max \`limit\` rækker (default ${DEFAULT_PAGE_LIMIT}, hard-cap ${MAX_PAGE_LIMIT}); ` +
  "envelope.data indeholder `total` (alle matchende rækker), `count` (rækker i dette svar), `limit`, " +
  "`offset`, `hasMore` og — når `hasMore=true` — `nextOffset` som agenten kan sende uændret for at hente næste side.";

export type PageMeta = {
  total: number;
  count: number;
  limit: number;
  offset: number;
  hasMore: boolean;
  nextOffset?: number;
};

export type PaginationArgs = {
  limit?: number;
  offset?: number;
};

/**
 * Slicer `rows` efter `limit`/`offset` og returnerer paginations-metadata.
 *
 * Bemærk: vi slicer i tool-laget i stedet for at presse limit/offset
 * ned i SQL — det holder kerne-funktionerne uændret og blast radius
 * lille. For tabeller med titusinder af rækker bør pagination flyttes
 * ned i SQL i en senere PR; men selv da forbliver _kontrakten_ (felter,
 * defaults, default-cap) i denne fil.
 */
export function applyPagination<T>(rows: readonly T[], args: PaginationArgs): {
  pageRows: T[];
  meta: PageMeta;
} {
  const total = rows.length;
  const rawLimit = typeof args.limit === "number" && Number.isFinite(args.limit)
    ? Math.floor(args.limit)
    : DEFAULT_PAGE_LIMIT;
  const limit = Math.max(1, Math.min(MAX_PAGE_LIMIT, rawLimit));
  const rawOffset = typeof args.offset === "number" && Number.isFinite(args.offset)
    ? Math.floor(args.offset)
    : 0;
  const offset = Math.max(0, rawOffset);

  const pageRows = rows.slice(offset, offset + limit);
  const count = pageRows.length;
  const hasMore = offset + count < total;
  const meta: PageMeta = { total, count, limit, offset, hasMore };
  if (hasMore) meta.nextOffset = offset + count;
  return { pageRows, meta };
}
