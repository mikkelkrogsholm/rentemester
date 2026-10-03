import { expect, test, vi } from "bun:test";
import { render, screen } from "@testing-library/react";
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
  stubGlobal("sessionStorage", { getItem: () => null, setItem: () => { throw new DOMException("Denied", "SecurityError"); } });
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
