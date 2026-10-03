import { expect, test, vi } from "bun:test";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MutationMemoryProvider, useMutationBlock } from "./mutation-memory";

function Controls({ name, operation = "invoice", write }: { name: string; operation?: string; write?: () => void }) {
  const memory = useMutationBlock(operation);
  return <><button type="button" onClick={memory.block}>Block {name}</button><button type="button" onClick={memory.release}>Release {name}</button><button type="button" disabled={memory.blocked} onClick={() => { if (!memory.isBlocked()) write?.(); }}>Write {name}</button></>;
}

test("an already-mounted independent tab observes an uncertain operation before another write", async () => {
  const write = vi.fn();
  render(<><MutationMemoryProvider><Controls name="A" /></MutationMemoryProvider><MutationMemoryProvider><Controls name="B" write={write} /></MutationMemoryProvider></>);
  await userEvent.click(screen.getByRole("button", { name: "Block A" }));
  expect(screen.getByRole("button", { name: "Write B" })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Write B" }));
  expect(write).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Release B" }));
  expect(screen.getByRole("button", { name: "Write A" })).toBeEnabled();
});

test("separate operation keys survive sibling updates, native storage events, and provider remounts", async () => {
  const first = render(<><MutationMemoryProvider><Controls name="A" operation="invoice" /></MutationMemoryProvider><MutationMemoryProvider><Controls name="B" operation="journal" /></MutationMemoryProvider></>);
  await userEvent.click(screen.getByRole("button", { name: "Block A" }));
  await userEvent.click(screen.getByRole("button", { name: "Block B" }));
  await userEvent.click(screen.getByRole("button", { name: "Release A" }));
  expect(screen.getByRole("button", { name: "Write B" })).toBeDisabled();
  first.unmount();
  render(<MutationMemoryProvider><Controls name="C" operation="journal" /></MutationMemoryProvider>);
  expect(screen.getByRole("button", { name: "Write C" })).toBeDisabled();
  const blockerKey = Object.keys(localStorage).find(key => key.startsWith("rentemester:uncertain-operations:v2:") && !key.endsWith(":probe"))!;
  act(() => { localStorage.removeItem(blockerKey); window.dispatchEvent(new StorageEvent("storage", { key: blockerKey })); });
  expect(screen.getByRole("button", { name: "Write C" })).toBeEnabled();
});

test("a user's blocker survives re-login and cannot be released by another user", async () => {
  const first = render(<MutationMemoryProvider scope="user:alice"><Controls name="Alice" /></MutationMemoryProvider>);
  await userEvent.click(screen.getByRole("button", { name: "Block Alice" }));
  first.unmount();
  const second = render(<MutationMemoryProvider scope="user:bob"><Controls name="Bob" /></MutationMemoryProvider>);
  expect(screen.getByRole("button", { name: "Write Bob" })).toBeEnabled();
  await userEvent.click(screen.getByRole("button", { name: "Release Bob" }));
  second.unmount();
  render(<MutationMemoryProvider scope="user:alice"><Controls name="Alice" /></MutationMemoryProvider>);
  expect(screen.getByRole("button", { name: "Write Alice" })).toBeDisabled();
});

test("unowned legacy blockers quarantine every user until explicit reconciliation", () => {
  // Opaque identifier for 'invoice' from the v1 storage format.
  sessionStorage.setItem("rentemester:uncertain-operations:v1", '["1519411472:1452965204"]');
  const first = render(<MutationMemoryProvider scope="user:bob"><Controls name="Legacy" /></MutationMemoryProvider>);
  expect(screen.getByRole("button", { name: "Write Legacy" })).toBeDisabled();
  expect(sessionStorage.getItem("rentemester:uncertain-operations:v1")).toBeNull();
  expect(Object.keys(localStorage).some(key => key.includes(":legacy:") && key.endsWith(":1519411472:1452965204"))).toBe(true);
  first.unmount();
  render(<MutationMemoryProvider scope="user:alice"><Controls name="Alice" /></MutationMemoryProvider>);
  expect(screen.getByRole("button", { name: "Write Alice" })).toBeDisabled();
});
