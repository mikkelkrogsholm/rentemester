export type TransactionRejection = { appliedRules?: string[]; errors?: string[] };

/** Throw inside the owning transaction; decode only after its rollback. */
export class TransactionRejectionError extends Error {
  readonly rejection: TransactionRejection;

  constructor(input: TransactionRejection & { errors: string[] }) {
    const rejection = {
      ...(input.appliedRules === undefined ? {} : { appliedRules: [...input.appliedRules] }),
      errors: [...input.errors],
    };
    // Keep the message codec for callers still handling the legacy Error form.
    super(JSON.stringify(rejection));
    this.rejection = rejection;
  }
}

/** Accept existing JSON-in-message errors as well as the structured form. */
export function decodeTransactionRejection(error: unknown): TransactionRejection | null {
  if (error instanceof TransactionRejectionError) return error.rejection;
  if (typeof error !== "object" || error === null || !("message" in error)) return null;
  try {
    const parsed: unknown = JSON.parse(String(error.message));
    if (typeof parsed !== "object" || parsed === null) return null;
    const stringArray = (value: unknown): value is string[] =>
      Array.isArray(value) && value.every(item => typeof item === "string");
    return {
      ...("appliedRules" in parsed && stringArray(parsed.appliedRules) ? { appliedRules: parsed.appliedRules } : {}),
      ...("errors" in parsed && stringArray(parsed.errors) ? { errors: parsed.errors } : {}),
    };
  } catch {
    return null;
  }
}
