/**
 * MCP-response-envelope.
 *
 * Alle Rentemester-MCP-tools svarer med samme shape så agenten kan
 * stole på ét fælles kontrakt-format uafhængigt af tool-kategori:
 *
 *   { ok, data?, errors[], appliedRules? }
 *
 * Reglerne (jf. docs/mcp-tool-surface.md):
 *  - `ok=true` ⇒ `data` er sat, `errors` er tom array
 *  - `ok=false` ⇒ `errors` er ikke-tom array af strings, `data` udelades
 *  - `appliedRules` listes altid for bogførings-tools (sporbarhed)
 */

export type Envelope<TData = Record<string, unknown>> = {
  ok: boolean;
  data?: TData;
  errors: string[];
  appliedRules?: string[];
};

/**
 * Wrapper et kerne-resultat i MCP-envelope-format.
 *
 * Kernens results har ofte shape `{ ok, errors, appliedRules, ...payload }`
 * — vi pakker `...payload` ud i `data` og bevarer `ok`, `errors`,
 * `appliedRules` på topniveau.
 */
export function wrapCoreResult<T extends { ok: boolean; errors?: unknown }>(
  result: T,
): Envelope {
  const { ok, errors, appliedRules, ...rest } = result as Record<string, unknown> & {
    ok: boolean;
    errors?: unknown;
    appliedRules?: unknown;
  };
  const envelope: Envelope = {
    ok: Boolean(ok),
    errors: normalizeErrors(errors),
  };
  if (Array.isArray(appliedRules) && appliedRules.length > 0) {
    envelope.appliedRules = appliedRules.map(String);
  }
  if (envelope.ok) envelope.data = rest;
  return envelope;
}

/**
 * Bygger en fejl-envelope uden at kalde kernen.
 *
 * Bruges fx når confirm-flag mangler, eller når input-validering fejler
 * før vi overhovedet rammer database/ledger.
 */
export function errorEnvelope(errors: string[] | string): Envelope {
  return {
    ok: false,
    errors: normalizeErrors(errors),
  };
}

/**
 * Bygger en succes-envelope direkte fra et data-objekt.
 */
export function successEnvelope<T extends Record<string, unknown>>(
  data: T,
  appliedRules?: string[],
): Envelope<T> {
  const envelope: Envelope<T> = {
    ok: true,
    data,
    errors: [],
  };
  if (appliedRules && appliedRules.length > 0) envelope.appliedRules = appliedRules.slice();
  return envelope;
}

/**
 * MCP `tools/call` skal returnere `{ content: [{ type: "text", text }] }`.
 * Vi serialiserer envelope'en som JSON og markerer `isError` så agenten
 * kan branche tidligt uden at parse body'en.
 */
export function envelopeToCallResult(envelope: Envelope): {
  content: Array<{ type: "text"; text: string }>;
  isError: boolean;
  structuredContent: Envelope;
} {
  return {
    content: [{ type: "text", text: JSON.stringify(envelope) }],
    isError: !envelope.ok,
    structuredContent: envelope,
  };
}

function normalizeErrors(value: unknown): string[] {
  if (!value) return [];
  if (Array.isArray(value)) return value.map((v) => String(v));
  return [String(value)];
}
