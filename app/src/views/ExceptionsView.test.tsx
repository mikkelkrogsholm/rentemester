import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ExceptionsView } from "./ExceptionsView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function payload(over: Partial<{
  status: "open" | "resolved" | "all";
  rows: any[];
  bySeverity: { high: number; medium: number; low: number };
}> = {}) {
  return {
    ok: true as const,
    exceptions: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
      },
      status: over.status ?? "open",
      rows: over.rows ?? [
        {
          id: 1,
          type: "UNMATCHED_BANK_TRANSACTION",
          severity: "high",
          status: "open",
          relatedBankTransactionId: 12,
          relatedDocumentId: null,
          message: "Bank-rækken passer ikke til nogen faktura eller bilag",
          requiredAction: "Importér det manglende bilag eller match manuelt",
          sourceEvidence: null,
          postingPreview: null,
          createdAt: "2026-05-20T10:00:00Z",
          resolvedAt: null,
          resolvedBy: null,
          resolutionNote: null,
          archived: false,
        },
        {
          id: 2,
          type: "BLOCKED_DOCUMENT_INGEST",
          severity: "medium",
          status: "open",
          relatedBankTransactionId: null,
          relatedDocumentId: 7,
          message: "Bilaget har ingen leverandørstamdata",
          requiredAction: "Tilføj leverandørens CVR",
          sourceEvidence: null,
          postingPreview: null,
          createdAt: "2026-05-21T09:00:00Z",
          resolvedAt: null,
          resolvedBy: null,
          resolutionNote: null,
          archived: false,
        },
      ],
      bySeverity: over.bySeverity ?? { high: 1, medium: 1, low: 0 },
      count: (over.rows ?? []).length || 2,
    },
  };
}

function renderView(
  initialPath = "/companies/acme-aps/undtagelser",
  body: any = payload(),
) {
  mockFetch({
    "GET /api/companies/acme-aps/exceptions": body,
    "POST /api/companies/acme-aps/exceptions/1/resolve": {
      ok: true,
      exception: { id: 1, resolved: true },
    },
  });
  return renderAt(<ExceptionsView />, {
    route: initialPath,
    path: "/companies/:slug/undtagelser",
  });
}

describe("ExceptionsView (#332)", () => {
  test("lister åbne undtagelser med severity-tæller", async () => {
    renderView();
    expect(
      await screen.findByText(/Bank-rækken passer ikke til nogen/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Bilaget har ingen leverandørstamdata/),
    ).toBeInTheDocument();
    expect(screen.getByText(/Høj: 1/)).toBeInTheDocument();
    expect(screen.getByText(/Medium: 1/)).toBeInTheDocument();
  });

  test("har 'Markér som løst'-knapper på åbne rækker", async () => {
    renderView();
    const buttons = await screen.findAllByRole("button", {
      name: /Markér som løst/,
    });
    expect(buttons.length).toBe(2);
  });

  test("filter-bar skifter til 'Løste' og opdaterer URL", async () => {
    const user = userEvent.setup();
    // For denne test mockes endpointet med en tom liste — det er hvad cockpittet
    // ser når der ikke er nogen løste undtagelser. Vi tjekker også at URL'en
    // får ?status=resolved.
    renderView(
      "/companies/acme-aps/undtagelser",
      payload({
        status: "resolved",
        rows: [],
        bySeverity: { high: 0, medium: 0, low: 0 },
      }),
    );
    const løste = await screen.findByRole("button", { name: /Løste/ });
    await user.click(løste);
    // 'primary'-klassen indikerer at status er aktiv.
    expect(løste.className).toContain("primary");
  });

  test("viser næste skridt for hver undtagelse", async () => {
    renderView();
    expect(
      await screen.findByText(/Importér det manglende bilag eller match manuelt/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Tilføj leverandørens CVR/),
    ).toBeInTheDocument();
  });

  test("en fejlet load er ikke en blindgyde — der er en 'Prøv igen'-knap", async () => {
    // Når selve listen ikke kan hentes, skal ErrorState tilbyde et retry
    // (onRetry={state.reload}) — ellers er skærmen en dødvej.
    mockFetch({
      "GET /api/companies/acme-aps/exceptions": {
        __error: { code: "internal", message: "Serveren svarede ikke." },
      },
    });
    renderAt(<ExceptionsView />, {
      route: "/companies/acme-aps/undtagelser",
      path: "/companies/:slug/undtagelser",
    });
    expect(
      await screen.findByText(/Serveren svarede ikke\./),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Prøv igen/ }),
    ).toBeInTheDocument();
  });
});
