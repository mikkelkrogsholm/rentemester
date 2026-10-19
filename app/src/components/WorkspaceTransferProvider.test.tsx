import { expect, test } from "bun:test";
import { useState } from "react";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderAt } from "../test/render";
import { Context, type Lifecycle } from "../lib/workspace-transfer-context";
import { WorkspaceReadOnlyGuard } from "./WorkspaceTransferProvider";
import { Dialog } from "./ui";

test("mode changes preserve open review, state and in-flight unknown outcome handler", async () => {
  let complete!: () => void;
  const pending = new Promise<void>(resolve => { complete = resolve; });
  function Review() {
    const [open, setOpen] = useState(false), [confirmed, setConfirmed] = useState(false), [outcome, setOutcome] = useState("");
    return <><button onClick={() => setOpen(true)}>Open review</button><a href="/reports">Read report</a>
      {open && <Dialog title="Reviewed send" onClose={() => setOpen(false)}>
        <label><input type="checkbox" checked={confirmed} onChange={e => setConfirmed(e.target.checked)} />Reviewed original</label>
        <button onClick={() => void pending.then(() => setOutcome("Unknown outcome retained"))}>Confirm send</button>
        {outcome && <p role="alert">{outcome}</p>}
      </Dialog>}
    </>;
  }
  function Harness() {
    const [writable, setWritable] = useState(true);
    const lifecycle: Lifecycle = { enabled: true, writable, scope: "synthetic", transfer: null, error: writable ? "" : "Status unavailable", loading: false, refresh: async () => {}, adopt: () => {} };
    return <Context.Provider value={lifecycle}><button onClick={() => setWritable(v => !v)}>Change mode</button><WorkspaceReadOnlyGuard><Review /></WorkspaceReadOnlyGuard></Context.Provider>;
  }
  renderAt(<Harness />, { route: "/companies/synthetic/fakturaer" });
  await userEvent.click(screen.getByRole("button", { name: "Open review" }));
  const dialog = screen.getByRole("dialog", { name: "Reviewed send" });
  const checkbox = within(dialog).getByRole("checkbox", { name: "Reviewed original" });
  await userEvent.click(checkbox);
  await userEvent.click(within(dialog).getByRole("button", { name: "Confirm send" }));
  await userEvent.click(screen.getByRole("button", { name: "Change mode" }));
  expect(screen.getByRole("dialog", { name: "Reviewed send" })).toBe(dialog);
  expect(checkbox).toBeChecked(); expect(checkbox).toBeDisabled();
  expect(within(dialog).getByRole("button", { name: "Confirm send" })).toBeDisabled();
  expect(screen.getByRole("link", { name: "Read report" })).toHaveAttribute("href", "/reports");
  complete(); await waitFor(() => expect(within(dialog).getByRole("alert")).toHaveTextContent("Unknown outcome retained"));
  await userEvent.click(screen.getByRole("button", { name: "Change mode" }));
  expect(screen.getByRole("dialog", { name: "Reviewed send" })).toBe(dialog);
  expect(checkbox).toBeChecked(); expect(checkbox).not.toBeDisabled();
  expect(within(dialog).getByRole("alert")).toHaveTextContent("Unknown outcome retained");
});
