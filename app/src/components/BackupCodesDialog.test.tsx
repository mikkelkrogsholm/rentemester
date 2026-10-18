import { beforeEach, describe, expect, test, vi } from "bun:test";
import { render, screen, within, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef, useState } from "react";
const fetchCodes = vi.fn();
vi.mock("../lib/auth-client", () => ({ authClient: { $fetch: fetchCodes } }));
import { BackupCodesDialog } from "./BackupCodesDialog";
const pool = Array.from({ length: 10 }, (_, i) => `ABCDE-0000${i}`);
function Harness() {
  const [open, setOpen] = useState(false),
    trigger = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button type="button" ref={trigger} onClick={() => setOpen(true)}>
        Sikkerhedskoder
      </button>
      {open && (
        <BackupCodesDialog
          returnFocusRef={trigger}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}
async function open() {
  render(<Harness />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Sikkerhedskoder" }));
  return user;
}
async function password(user: ReturnType<typeof userEvent.setup>) {
  await user.type(
    screen.getByLabelText("Adgangskode til sikkerhedskoder"),
    "private-entered-password",
  );
}
async function regenerate(user: ReturnType<typeof userEvent.setup>) {
  await password(user);
  await user.click(screen.getByRole("button", { name: "Opret nye koder" }));
  await user.click(
    within(
      screen.getByRole("dialog", { name: "Erstat sikkerhedskoder?" }),
    ).getByRole("button", { name: "Erstat koder" }),
  );
}
describe("password-bound backup code dialog", () => {
  beforeEach(() => {
    fetchCodes.mockReset();
    fetchCodes.mockResolvedValue({ data: { status: true, backupCodes: pool } });
  });
  test("opens without disclosure, view requires password, clears on close and reopen", async () => {
    const user = await open();
    expect(fetchCodes).not.toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: "Vis resterende koder" }),
    ).toBeDisabled();
    await password(user);
    await user.click(
      screen.getByRole("button", { name: "Vis resterende koder" }),
    );
    expect(await screen.findByText(pool[0]!)).toBeInTheDocument();
    expect(fetchCodes).toHaveBeenCalledWith("/two-factor/view-backup-codes", {
      method: "POST",
      body: { password: "private-entered-password" },
      retry: 0,
    });
    expect(
      screen.queryByLabelText("Adgangskode til sikkerhedskoder"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Luk og skjul" }));
    expect(screen.queryByText(pool[0]!)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Sikkerhedskoder" }));
    expect(
      screen.getByLabelText("Adgangskode til sikkerhedskoder"),
    ).toHaveValue("");
    expect(screen.queryByText(pool[0]!)).not.toBeInTheDocument();
    expect(fetchCodes).toHaveBeenCalledTimes(1);
  });
  test("regeneration is confirmation gated, cancellation sends nothing", async () => {
    const user = await open();
    await password(user);
    await user.click(screen.getByRole("button", { name: "Opret nye koder" }));
    expect(fetchCodes).not.toHaveBeenCalled();
    await user.click(
      within(
        screen.getByRole("dialog", { name: "Erstat sikkerhedskoder?" }),
      ).getByRole("button", { name: "Annullér" }),
    );
    expect(fetchCodes).not.toHaveBeenCalled();
  });
  test("confirmed regeneration discloses exactly ten fresh returned codes without retry", async () => {
    const user = await open();
    await regenerate(user);
    expect(await screen.findByText(pool[9]!)).toBeInTheDocument();
    expect(fetchCodes).toHaveBeenCalledWith(
      "/two-factor/generate-backup-codes",
      {
        method: "POST",
        body: { password: "private-entered-password" },
        retry: 0,
      },
    );
    expect(
      within(
        screen.getByRole("list", { name: "Resterende sikkerhedskoder" }),
      ).getAllByRole("listitem"),
    ).toHaveLength(10);
  });
  test("empty view is a valid exhausted pool without invented codes", async () => {
    fetchCodes.mockResolvedValue({ data: { status: true, backupCodes: [] } });
    const user = await open();
    await password(user);
    await user.click(
      screen.getByRole("button", { name: "Vis resterende koder" }),
    );
    expect(
      await screen.findByText(
        "Ingen sikkerhedskoder tilbage. Opret nye koder, før du får brug for dem.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("listitem")).not.toBeInTheDocument();
  });
  test.each([
    "false",
    "missing-status",
    "empty",
    "duplicate",
    "invalid",
    "too-many",
    "null",
  ])(
    "malformed regeneration %s withholds codes and reports uncertainty",
    async (kind) => {
      const data =
        kind === "false"
          ? { status: false, backupCodes: pool }
          : kind === "missing-status"
            ? { backupCodes: pool }
            : kind === "null"
              ? null
              : {
                  status: true,
                  backupCodes:
                    kind === "empty"
                      ? []
                      : kind === "duplicate"
                        ? [...pool.slice(0, 9), pool[0]]
                        : kind === "invalid"
                          ? [...pool.slice(0, 9), "invalid-private-code"]
                          : [...pool, "FGHIJ-00000"],
                };
      fetchCodes.mockResolvedValue({ data });
      const user = await open();
      await regenerate(user);
      expect(
        await screen.findByText(/Tidligere koder kan være erstattet/),
      ).toBeInTheDocument();
      expect(screen.queryByRole("list")).not.toBeInTheDocument();
      expect(
        screen.getByLabelText("Adgangskode til sikkerhedskoder"),
      ).toHaveValue("");
      expect(fetchCodes).toHaveBeenCalledTimes(1);
    },
  );
  test.each(["rejected", "transport"])(
    "uncertain regeneration %s never replays or claims unchanged old pool",
    async (kind) => {
      if (kind === "transport")
        fetchCodes.mockRejectedValue(new Error("private failure detail"));
      else
        fetchCodes.mockResolvedValue({
          error: { status: 503, message: "private failure detail" },
        });
      const user = await open();
      await regenerate(user);
      expect(
        await screen.findByText(/Tidligere koder kan være erstattet/),
      ).toBeInTheDocument();
      expect(
        screen.queryByText("private failure detail"),
      ).not.toBeInTheDocument();
      expect(fetchCodes).toHaveBeenCalledTimes(1);
    },
  );
  test("unsupported native view never falls back to source serverOnly userId or regenerates", async () => {
    fetchCodes.mockResolvedValue({ error: { status: 404 } });
    const user = await open();
    await password(user);
    await user.click(
      screen.getByRole("button", { name: "Vis resterende koder" }),
    );
    expect(
      await screen.findByText(
        "Sikkerhedskoder er ikke tilgængelige på denne server.",
      ),
    ).toBeInTheDocument();
    expect(fetchCodes).toHaveBeenCalledTimes(1);
    expect(fetchCodes.mock.calls[0]![1].body).toEqual({
      password: "private-entered-password",
    });
  });
  test("pagehide clears a visible pool and password without another request", async () => {
    const user = await open();
    await password(user);
    await user.click(
      screen.getByRole("button", { name: "Vis resterende koder" }),
    );
    await screen.findByText(pool[0]!);
    act(() => window.dispatchEvent(new Event("pagehide")));
    expect(screen.queryByText(pool[0]!)).not.toBeInTheDocument();
    expect(
      screen.getByLabelText("Adgangskode til sikkerhedskoder"),
    ).toHaveValue("");
    expect(fetchCodes).toHaveBeenCalledTimes(1);
  });
  test("late regeneration response after leaving page is discarded with explicit unknown guidance", async () => {
    let resolve!: (v: unknown) => void;
    fetchCodes.mockImplementation(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    const user = await open();
    await regenerate(user);
    expect(fetchCodes).toHaveBeenCalledTimes(1);
    act(() => window.dispatchEvent(new Event("pagehide")));
    await act(async () =>
      resolve({ data: { status: true, backupCodes: pool } }),
    );
    expect(screen.queryByText(pool[0]!)).not.toBeInTheDocument();
    expect(
      screen.getByText(/Tidligere koder kan være erstattet/),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText("Adgangskode til sikkerhedskoder"),
    ).toHaveValue("");
    expect(fetchCodes).toHaveBeenCalledTimes(1);
  });
});
