import { afterEach, describe, expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { renderAt } from "../test/render";
import { restoreGlobals } from "../test/globals";

vi.mock("../lib/auth-context", () => ({
  useOptionalAuth: () => ({ hosted: false }),
  useAuth: () => ({
    context: {
      workspaceRole: "workspace_owner",
      companies: [{ slug: "synthetic-company", name: "Synthetic Company", role: "owner", archived: false }],
    },
  }),
}));

const { WorkspaceAccessView } = await import("./WorkspaceAccessView");

afterEach(() => restoreGlobals());

describe("WorkspaceAccessView", () => {
  test("invites one person to one company with the selected minimum role", async () => {
    mockFetch({
      "GET /api/workspace/invitations": { invitations: [] },
      "GET /api/workspace/members": { members: [] },
      "POST /api/workspace/invitations": {
        invitation: {
          invitationId: "synthetic-id", email: "reader@example.test",
          workspaceRole: "member", companySlug: "synthetic-company", companyRole: "reader",
          expiresAt: "2026-08-30T00:00:00.000Z", status: "delivery_confirmed",
          userId: null, createdAt: "2026-08-23T00:00:00.000Z",
        },
      },
    });
    renderAt(<WorkspaceAccessView />, { route: "/adgang", path: "/adgang" });
    await screen.findByText("Ingen invitationer endnu.");
    await userEvent.type(screen.getByLabelText("E-mail"), "reader@example.test");
    await userEvent.selectOptions(screen.getByLabelText("Rolle"), "reader");
    await userEvent.click(screen.getByRole("button", { name: "Send invitation" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Invitationen er sendt");
    const call = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) =>
      String(url) === "/api/workspace/invitations" && init?.method === "POST"
    );
    expect(JSON.parse(String((call![1] as RequestInit).body))).toEqual({
      email: "reader@example.test",
      workspaceRole: "member",
      companySlug: "synthetic-company",
      companyRole: "reader",
    });
  });

  test("confirms a company role change for an existing member", async () => {
    mockFetch({
      "GET /api/workspace/invitations": { invitations: [] },
      "GET /api/workspace/members": { members: [{
        userId: "member-1", name: "Test Member", email: "member@example.test",
        emailVerified: true, twoFactorEnabled: true, accessReady: true,
        workspaceRole: "member",
        memberships: [{
          companySlug: "synthetic-company", companyName: "Synthetic Company",
          role: "reader", archived: false,
        }],
      }] },
      "POST /api/workspace/members/company": {},
    });
    renderAt(<WorkspaceAccessView />, { route: "/adgang", path: "/adgang" });
    await screen.findByText("Test Member");
    await userEvent.selectOptions(screen.getByLabelText("Bruger"), "member-1");
    await userEvent.selectOptions(screen.getByLabelText("Virksomhedsrolle"), "bookkeeper");
    await userEvent.click(screen.getByRole("button", { name: "Gem virksomhedsrolle" }));
    expect(screen.getByRole("dialog", { name: "Bekræft adgangsændring" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Gennemfør ændring" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Adgangen er opdateret");
    const call = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) =>
      String(url) === "/api/workspace/members/company" && init?.method === "POST"
    );
    expect(JSON.parse(String((call![1] as RequestInit).body))).toEqual({
      action: "grant", userId: "member-1",
      companySlug: "synthetic-company", role: "bookkeeper",
    });
  });
});
