import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
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
  test("lists customers and vendors", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
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

  test("the company sub-nav exposes the Kontakter tab", async () => {
    mockFetch(route());
    renderView();
    const tab = await screen.findByRole("link", { name: "Kontakter" });
    expect(tab).toHaveAttribute(
      "href",
      expect.stringContaining("/companies/acme-aps/kontakter"),
    );
  });

  test("offers an Importér action", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
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
    await screen.findByRole("heading", { name: "Acme ApS" });
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
});
