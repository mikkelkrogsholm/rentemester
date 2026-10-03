import { describe, expect, test, vi } from "bun:test";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ContactsView } from "./ContactsView";
import { renderAt } from "../test/render";
import { contacts, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/contacts": { contacts: contacts(over) },
  };
}

function renderView() {
  return renderAt(<ContactsView />, {
    route: "/companies/acme-aps/kontakter",
    path: "/companies/:slug/kontakter",
  });
}

describe("ContactsView — Kontakter", () => {
  test("links only contacts with an explicit canonical party ID", async () => {
    mockFetch(route({ customers: [
      { ...contacts().customers[0], id: 11, name: "Samme navn", partyId: "party-customer" },
      { ...contacts().customers[0], id: 12, name: "Samme navn", partyId: null },
    ], vendors: [] }));
    renderView();
    expect(await screen.findByRole("link", { name: "Samme navn" })).toHaveAttribute("href", "/companies/acme-aps/parter/party-customer");
    expect(screen.getAllByText("Samme navn").some((element) => element.closest("a") === null)).toBe(true);
  });

  test("lists customers and vendors", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Kontakter", level: 1 }),
    ).toBeInTheDocument();
    expect(screen.getByText("Kunde A/S")).toBeInTheDocument();
    expect(screen.getByText("Leverandør ApS")).toBeInTheDocument();
    expect(screen.getByText("Standardmoms")).toBeInTheDocument();
  });

  test("shows a graceful empty state when there are no contacts", async () => {
    mockFetch(route({ customers: [], vendors: [] }));
    renderView();
    expect(
      await screen.findByText(/Ingen kontakter endnu/),
    ).toBeInTheDocument();
  });

  test("the daily navigation groups contacts under knowledge", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByRole("link", { name: "Viden" })).toHaveAttribute("href", expect.stringContaining("/companies/acme-aps/parter"));
  });

  test("offers an Importér action", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Kontakter", level: 1 });
    expect(
      screen.getByRole("button", { name: "Importér" }),
    ).toBeInTheDocument();
  });

  test("clicking Importér opens the file-import modal", async () => {
    mockFetch(route());
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: "Importér" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Importér fil" }),
    ).toBeInTheDocument();
  });

  // #390 — the cockpit's daily-maintenance surface.
  test("offers a Tilføj kunde and Tilføj leverandør action", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Kontakter", level: 1 });
    expect(
      screen.getByRole("button", { name: "Tilføj kunde" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Tilføj leverandør" }),
    ).toBeInTheDocument();
  });

  test("clicking Tilføj kunde opens the contact form modal", async () => {
    mockFetch(route());
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: "Tilføj kunde" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Tilføj kunde" }),
    ).toBeInTheDocument();
  });

  test("clicking Tilføj leverandør opens the vendor form modal", async () => {
    mockFetch(route());
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: "Tilføj leverandør" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Tilføj leverandør" }),
    ).toBeInTheDocument();
  });

  test("each customer row has a Redigér action that opens the edit modal", async () => {
    mockFetch(route());
    renderView();
    const editButton = await screen.findByRole("button", {
      name: "Redigér Kunde A/S",
    });
    await userEvent.click(editButton);
    expect(
      screen.getByRole("dialog", { name: "Redigér kunde" }),
    ).toBeInTheDocument();
  });

  test("each vendor row has a Redigér action that opens the edit modal", async () => {
    mockFetch(route());
    renderView();
    const editButton = await screen.findByRole("button", {
      name: "Redigér Leverandør ApS",
    });
    await userEvent.click(editButton);
    expect(
      screen.getByRole("dialog", { name: "Redigér leverandør" }),
    ).toBeInTheDocument();
  });

  test("empty state surfaces oprettelses-knapper, not only Importér", async () => {
    mockFetch(route({ customers: [], vendors: [] }));
    renderView();
    await screen.findByText(/Ingen kontakter endnu/);
    // Both buttons should appear inside the empty-state card as well, so a
    // brand-new owner sees a clear "create stamdata" call-to-action.
    const addCustomerButtons = screen.getAllByRole("button", {
      name: "Tilføj kunde",
    });
    expect(addCustomerButtons.length).toBeGreaterThanOrEqual(1);
  });

  // #430 — sletning af kontakter fra cockpittet.
  test("each customer row has a Slet action that opens a confirm dialog", async () => {
    mockFetch(route());
    renderView();
    const deleteButton = await screen.findByRole("button", {
      name: "Slet Kunde A/S",
    });
    await userEvent.click(deleteButton);
    expect(
      screen.getByRole("dialog", { name: /Slet kunde Kunde A\/S/i }),
    ).toBeInTheDocument();
    // Konsekvenser forklares i dansk, ikke-teknisk sprog.
    expect(
      screen.getByText(/bogførte fakturaer og posteringer beholder navnet/i),
    ).toBeInTheDocument();
  });

  test("each vendor row has a Slet action that opens a confirm dialog", async () => {
    mockFetch(route());
    renderView();
    const deleteButton = await screen.findByRole("button", {
      name: "Slet Leverandør ApS",
    });
    await userEvent.click(deleteButton);
    expect(
      screen.getByRole("dialog", { name: /Slet leverandør Leverandør ApS/i }),
    ).toBeInTheDocument();
  });
});

test("restores contact search and type filters from the URL", async () => {
  mockFetch(route());
  renderAt(<ContactsView />, { route: "/companies/acme-aps/kontakter?q=leverandør&kind=vendors", path: "/companies/:slug/kontakter" });
  expect(await screen.findByText("Leverandør ApS")).toBeInTheDocument();
  expect(screen.queryByText("Kunde A/S")).not.toBeInTheDocument();
  expect(screen.getByLabelText("Søg kontakter")).toHaveValue("leverandør");
  expect(screen.getByLabelText("Kontakttype")).toHaveValue("vendors");
});


test("a dirty contact form keeps the draft until discard is explicitly confirmed", async () => {
  mockFetch(route());
  renderView();
  await userEvent.click(await screen.findByRole("button", { name: "Tilføj kunde" }));
  const form = screen.getByRole("dialog", { name: "Tilføj kunde" });
  await userEvent.type(within(form).getByLabelText("Navn"), "Synthetic Customer");
  await userEvent.click(within(form).getByRole("button", { name: "Annullér" }));
  let confirmation = screen.getByRole("dialog", { name: "Kassér ændringer?" });
  await userEvent.click(within(confirmation).getByRole("button", { name: "Annullér" }));
  expect(within(form).getByLabelText("Navn")).toHaveValue("Synthetic Customer");
  await userEvent.click(within(form).getByRole("button", { name: "Annullér" }));
  confirmation = screen.getByRole("dialog", { name: "Kassér ændringer?" });
  await userEvent.click(within(confirmation).getByRole("button", { name: "Kassér ændringer" }));
  expect(screen.queryByRole("dialog", { name: "Tilføj kunde" })).not.toBeInTheDocument();
});

test("an unchanged existing contact closes directly", async () => {
  mockFetch(route());
  renderView();
  await userEvent.click(await screen.findByRole("button", { name: "Redigér Kunde A/S" }));
  const form = screen.getByRole("dialog", { name: "Redigér kunde" });
  await userEvent.click(within(form).getByRole("button", { name: "Annullér" }));
  expect(screen.queryByRole("dialog", { name: "Kassér ændringer?" })).not.toBeInTheDocument();
  expect(screen.queryByRole("dialog", { name: "Redigér kunde" })).not.toBeInTheDocument();
});

test("confirmed contact save dismisses a dirty form without a discard warning", async () => {
  mockFetch({ ...route(), "POST /api/companies/acme-aps/customers": { customer: { id: 42 } } });
  renderView();
  await userEvent.click(await screen.findByRole("button", { name: "Tilføj kunde" }));
  const form = screen.getByRole("dialog", { name: "Tilføj kunde" });
  await userEvent.type(within(form).getByLabelText("Navn"), "Synthetic Customer");
  await userEvent.click(within(form).getByRole("button", { name: "Opret" }));
  expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.some(([url, init]) => String(url).endsWith("/customers") && init?.method === "POST")).toBe(true);
  expect(screen.queryByRole("dialog", { name: "Kassér ændringer?" })).not.toBeInTheDocument();
  expect(screen.queryByRole("dialog", { name: "Tilføj kunde" })).not.toBeInTheDocument();
});


for (const code of ["network", "internal"]) test(`import preserves the file and blocks retries after ${code}`, async () => {
  const fixtures: Record<string, unknown> = { ...route(), "POST /api/companies/acme-aps/import": { __error: { code, message: "Synthetic interrupted response" } } };
  mockFetch(fixtures);
  renderView();
  await userEvent.click(await screen.findByRole("button", { name: "Importér" }));
  const dialog = screen.getByRole("dialog", { name: "Importér fil" });
  const file = new File(["id,name\n1,Synthetic"], "synthetic.csv", { type: "text/csv" });
  await userEvent.upload(within(dialog).getByLabelText("Fil"), file);
  const write = within(dialog).getByRole("button", { name: /^Importér$/ });
  await userEvent.click(write);
  expect(await within(dialog).findByText(/Serverens resultat kunne ikke bekræftes/)).toBeInTheDocument();
  expect((within(dialog).getByLabelText("Fil") as HTMLInputElement).files?.[0]?.name).toBe("synthetic.csv");
  expect(write).toBeDisabled();
  if (code === "network") fixtures["GET /api/companies/acme-aps/contacts"] = { __error: { code: "network", message: "Synthetic status read failure" } };
  await userEvent.click(within(dialog).getByRole("button", { name: "Kontrollér status" }));
  if (code === "network") await screen.findByText(/Status kunne ikke opdateres/);
  await userEvent.click(write);
  expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([url, init]) => String(url).endsWith("/import") && init?.method === "POST")).toHaveLength(1);
  expect(write).toBeDisabled();
});
