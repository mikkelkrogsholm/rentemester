import { expect, test, vi } from "bun:test";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { MutationMemoryProvider } from "./mutation-memory";
import { useMutationOutcome } from "./useMutationOutcome";
import { ApiError } from "./api";
import { stubGlobal } from "../test/globals";

test("a financial form cannot bypass an uncertain write by remounting or changing year filters", async () => {
  sessionStorage.clear();
  const write = vi.fn(async () => { throw new ApiError("network", "Svar mistet", 0); });
  const read = vi.fn(async () => {});
  function Form() {
    const outcome = useMutationOutcome(read);
    return <><button type="button" disabled={outcome.blocked} onClick={() => { if (!outcome.isBlocked()) void write().catch(outcome.reject).catch(() => {}); }}>Bogfør</button>{outcome.feedback}</>;
  }
  function view(path: string) { return <MemoryRouter initialEntries={[path]}><MutationMemoryProvider><Form /></MutationMemoryProvider></MemoryRouter>; }
  const first = render(view("/companies/synthetic/finans?year=2026"));
  await userEvent.click(screen.getByRole("button", { name: "Bogfør" }));
  await screen.findByRole("alert");
  first.unmount();
  render(view("/companies/synthetic/finans?year=2025"));
  expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Kontrollér status" }));
  expect(read).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
  expect(write).toHaveBeenCalledTimes(1);
});

test("denied browser storage blocks writes before an attempt and cannot claim reload protection", async () => {
  sessionStorage.clear();
  stubGlobal("localStorage", { getItem: () => null, setItem: () => { throw new DOMException("Denied", "SecurityError"); } });
  const write = vi.fn();
  function Form() {
    const outcome = useMutationOutcome();
    return <><button type="button" disabled={outcome.blocked} onClick={write}>Udsted</button>{outcome.feedback}</>;
  }
  const view = <MutationMemoryProvider><Form /></MutationMemoryProvider>;
    const first = render(view);
    expect(screen.getByRole("button", { name: "Udsted" })).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent("browseren ikke kan gemme");
    expect(screen.getByRole("button", { name: "Resultatet er afklaret" })).toBeDisabled();
    first.unmount();
    render(view);
    await userEvent.click(screen.getByRole("button", { name: "Udsted" }));
    expect(write).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Udsted" })).toBeDisabled();
});


test("storage becoming unwritable after mount prevents either provider from sending a request", async () => {
  const write = vi.fn(async () => {});
  function Form({ name }: { name: string }) {
    const outcome = useMutationOutcome();
    return <><button type="button" disabled={outcome.blocked} onClick={() => { void outcome.run(write).catch(() => {}); }}>Send {name}</button>{outcome.feedback}</>;
  }
  render(<><MutationMemoryProvider><Form name="A" /></MutationMemoryProvider><MutationMemoryProvider><Form name="B" /></MutationMemoryProvider></>);
  const storage = localStorage;
  stubGlobal("localStorage", { getItem: storage.getItem.bind(storage), setItem: () => { throw new DOMException("Full", "QuotaExceededError"); }, removeItem: storage.removeItem.bind(storage) });
  await userEvent.click(screen.getByRole("button", { name: "Send A" }));
  await userEvent.click(screen.getByRole("button", { name: "Send B" }));
  expect(write).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Send A" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Send B" })).toBeDisabled();
});

test("an unknown response retains the pre-request blocker even if storage fails during the request", async () => {
  let rejectResponse!: (error: unknown) => void;
  const response = new Promise<void>((_, reject) => { rejectResponse = reject; });
  const write = vi.fn(() => response);
  function Form({ name }: { name: string }) {
    const outcome = useMutationOutcome();
    return <><button type="button" disabled={outcome.blocked} onClick={() => { void outcome.run(write).catch(() => {}); }}>Send {name}</button>{outcome.feedback}</>;
  }
  render(<><MutationMemoryProvider><Form name="A" /></MutationMemoryProvider><MutationMemoryProvider><Form name="B" /></MutationMemoryProvider></>);
  await userEvent.click(screen.getByRole("button", { name: "Send A" }));
  expect(write).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Send B" })).toBeDisabled();
  const storage = localStorage;
  stubGlobal("localStorage", { getItem: storage.getItem.bind(storage), setItem: () => { throw new DOMException("Denied", "SecurityError"); }, removeItem: storage.removeItem.bind(storage) });
  await act(async () => { rejectResponse(new ApiError("internal", "Svar mistet", 500)); await response.catch(() => {}); });
  await userEvent.click(screen.getByRole("button", { name: "Send B" }));
  expect(write).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Send B" })).toBeDisabled();
});
