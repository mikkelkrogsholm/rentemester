import { useState } from "react";
import { describe, expect, test, vi } from "bun:test";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConfirmDialog } from "./ConfirmDialog";
import { ApiError } from "../lib/api";
import { MutationMemoryProvider } from "../lib/mutation-memory";

function noop() {}

describe("ConfirmDialog", () => {
  test("cannot repeat an unknown operation by closing and reopening its confirmation", async () => {
    sessionStorage.clear();
    const write = vi.fn(async () => { throw new ApiError("network", "Svar mistet", 0); });
    const refresh = vi.fn(async () => {});
    function Harness() {
      const [open, setOpen] = useState(false);
      return <MutationMemoryProvider><button type="button" onClick={() => setOpen(true)}>Åbn handling</button>{open && <ConfirmDialog title="Afstem faktura" operationKey="invoice-42" body="x" confirmLabel="Afstem" onConfirm={write} onRefresh={refresh} onClose={() => setOpen(false)} />}</MutationMemoryProvider>;
    }
    render(<Harness />);
    await userEvent.click(screen.getByRole("button", { name: "Åbn handling" }));
    await userEvent.click(screen.getByRole("button", { name: "Afstem" }));
    await screen.findByText("Svar mistet");
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    await userEvent.click(screen.getByRole("button", { name: "Åbn handling" }));
    expect(screen.getByRole("button", { name: "Afstem" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Kontrollér status" }));
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Afstem" })).toBeDisabled();
    expect(write).toHaveBeenCalledTimes(1);
  });

  test("remount keeps an uncertain operation blocked until explicit reconciliation without sending a write", async () => {
    sessionStorage.clear();
    const write = vi.fn(async () => { throw new ApiError("network", "Svar mistet", 0); });
    const dialog = <MutationMemoryProvider><ConfirmDialog title="Send faktura" operationKey="invoice-99" body="x" confirmLabel="Send" onConfirm={write} onClose={noop} /></MutationMemoryProvider>;
    const first = render(dialog);
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByText("Svar mistet");
    const stored = JSON.stringify(Object.entries(localStorage));
    expect(stored).not.toContain("invoice-99");
    expect(stored).not.toContain("Send faktura");
    first.unmount();
    render(dialog);
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Resultatet er afklaret" }));
    await userEvent.click(screen.getByRole("button", { name: "Behold blokering" }));
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Resultatet er afklaret" }));
    await userEvent.click(screen.getByRole("button", { name: "Jeg har kontrolleret resultatet" }));
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
    expect(write).toHaveBeenCalledTimes(1);
    expect(Object.keys(localStorage).filter(key => key.startsWith("rentemester:uncertain-operations:v2:") && !key.endsWith(":probe"))).toEqual([]);
  });

  test("validates an email before calling the action", async () => {
    const write = vi.fn(async () => {});
    render(<ConfirmDialog title="Send faktura" body="x" confirmLabel="Send" noteLabel="Modtager" noteInputType="email" onConfirm={write} onClose={noop} />);
    await userEvent.type(screen.getByLabelText("Modtager"), "invalid");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(write).not.toHaveBeenCalled();
    await userEvent.clear(screen.getByLabelText("Modtager"));
    await userEvent.type(screen.getByLabelText("Modtager"), "recipient@example.test");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(write).toHaveBeenCalledWith("recipient@example.test");
  });

  test("asks before discarding an edited confirmation note", async () => {
    const close = vi.fn();
    render(<ConfirmDialog title="Kreditér" body="x" confirmLabel="Kreditér nu" noteLabel="Begrundelse" onConfirm={async () => {}} onClose={close} />);
    await userEvent.type(screen.getByLabelText("Begrundelse"), "Varer returneret");
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    expect(close).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Kassér ændringer?" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Kassér ændringer" }));
    expect(close).toHaveBeenCalledTimes(1);
  });
  test("renders the title, body and confirm label", () => {
    render(
      <ConfirmDialog
        title="Løs opgave"
        body={<p>Markér opgaven som løst.</p>}
        confirmLabel="Løs opgave"
        onConfirm={async () => {}}
        onClose={noop}
      />,
    );
    expect(screen.getByRole("dialog", { name: "Løs opgave" })).toBeInTheDocument();
    expect(screen.getByText("Markér opgaven som løst.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Løs opgave" })).toBeInTheDocument();
  });

  test("passes the note text to onConfirm", async () => {
    const onConfirm = vi.fn(async () => {});
    render(
      <ConfirmDialog
        title="Løs opgave"
        body="x"
        confirmLabel="Løs"
        noteLabel="Note"
        onConfirm={onConfirm}
        onClose={noop}
      />,
    );
    await userEvent.type(screen.getByLabelText("Note"), "Afstemt manuelt");
    await userEvent.click(screen.getByRole("button", { name: "Løs" }));
    expect(onConfirm).toHaveBeenCalledWith("Afstemt manuelt");
  });

  test("closes via onClose after a successful confirm", async () => {
    const onClose = vi.fn();
    render(
      <ConfirmDialog
        title="Løs opgave"
        body="x"
        confirmLabel="Løs"
        onConfirm={async () => {}}
        onClose={onClose}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Løs" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  test("Annullér closes without confirming", async () => {
    const onConfirm = vi.fn(async () => {});
    const onClose = vi.fn();
    render(
      <ConfirmDialog
        title="Løs opgave"
        body="x"
        confirmLabel="Løs"
        onConfirm={onConfirm}
        onClose={onClose}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    expect(onClose).toHaveBeenCalled();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  test("a 409 conflict is rendered as a kind lock banner, modal stays open", async () => {
    const onClose = vi.fn();
    render(
      <ConfirmDialog
        title="Løs opgave"
        body="x"
        confirmLabel="Løs"
        onConfirm={async () => {
          throw new ApiError("conflict", "Bogføring er låst: backup overskredet.", 409);
        }}
        onClose={onClose}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Løs" }));
    expect(await screen.findByText("Bogføringen er låst")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  test("traps Tab focus inside the dialog (#UI-12)", async () => {
    render(
      <ConfirmDialog
        title="Løs opgave"
        body="x"
        confirmLabel="Løs"
        onConfirm={async () => {}}
        onClose={noop}
      />,
    );
    const confirm = screen.getByRole("button", { name: "Løs" });
    const cancel = screen.getByRole("button", { name: "Annullér" });
    // Focus starts on the confirm button (the last focusable). Tabbing forward
    // from the last element wraps to the first (Annullér), not out to <body>.
    expect(confirm).toHaveFocus();
    await userEvent.tab();
    expect(cancel).toHaveFocus();
    // Shift+Tab from the first wraps back to the last.
    await userEvent.tab({ shift: true });
    expect(confirm).toHaveFocus();
  });

  test("returns focus to the trigger element on close (#UI-12)", async () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            Åbn
          </button>
          {open && (
            <ConfirmDialog
              title="Løs opgave"
              body="x"
              confirmLabel="Løs"
              onConfirm={async () => {}}
              onClose={() => setOpen(false)}
            />
          )}
        </>
      );
    }
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "Åbn" });
    // Open from the trigger so the dialog captures it as the element to restore.
    await userEvent.click(trigger);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    // Closing returns focus to the trigger, not to <body>.
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  test("a non-conflict error is rendered as an error banner", async () => {
    render(
      <ConfirmDialog
        title="Løs opgave"
        body="x"
        confirmLabel="Løs"
        onConfirm={async () => {
          throw new ApiError("bad_request", "Ugyldig handling.", 400);
        }}
        onClose={noop}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Løs" }));
    expect(await screen.findByText("Ugyldig handling.")).toBeInTheDocument();
  });
});
