import { describe, expect, test } from "bun:test";
import { decodeTransactionRejection, TransactionRejectionError } from "../../src/core/transaction-rejection";

describe("transaction rejection compatibility", () => {
  test("structured rejection retains the legacy message and snapshots supplied arrays", () => {
    const input = { appliedRules: ["SYNTHETIC-RULE"], errors: ["synthetic refusal"] };
    const error = new TransactionRejectionError(input);
    const legacy = new Error(JSON.stringify(input));
    expect(error.message).toBe(legacy.message);
    expect(String(error)).toBe(String(legacy));
    input.errors.push("later mutation");
    expect(decodeTransactionRejection(error)).toEqual(decodeTransactionRejection(legacy));
  });

  test("legacy Error and message-bearing values retain independent errors/rules and empty arrays", () => {
    for (const data of [{ errors: ["legacy refusal"] }, { appliedRules: ["LEGACY-RULE"] }, { errors: [], appliedRules: [] }]) {
      expect(decodeTransactionRejection(new Error(JSON.stringify(data)))).toEqual(data);
      expect(decodeTransactionRejection({ message: JSON.stringify(data) })).toEqual(data);
    }
  });

  test("unknown failures keep their original fallback and cannot supply untyped rule/error lists", () => {
    for (const error of [new Error("disk failure"), "disk failure", null, new Error("null")]) {
      expect(decodeTransactionRejection(error)).toBeNull();
    }
    expect(decodeTransactionRejection(new Error('{"errors":"invalid","appliedRules":[1]}'))).toEqual({});
  });
});
