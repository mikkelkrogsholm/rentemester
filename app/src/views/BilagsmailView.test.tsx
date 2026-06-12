import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import { BilagsmailView } from "./BilagsmailView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function payload(over: Partial<{
  imapConfigured: boolean;
  imapStatus: any;
  mailAlias: string | null;
  inbox: any[];
}> = {}) {
  return {
    ok: true as const,
    bilagsmail: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
      },
      imapConfigured: over.imapConfigured ?? false,
      imapStatus: over.imapStatus ?? null,
      mailAlias: over.mailAlias ?? null,
      inbox: over.inbox ?? [],
    },
  };
}

function renderView(body = payload()) {
  mockFetch({ "GET /api/companies/acme-aps/bilagsmail": body });
  return renderAt(<BilagsmailView />, {
    route: "/companies/acme-aps/bilagsmail",
    path: "/companies/:slug/bilagsmail",
  });
}

describe("BilagsmailView (#348/#350/#351)", () => {
  test("tom-state: ikke konfigureret + ingen alias + tom inbox", async () => {
    renderView();
    expect(await screen.findByText(/Mail-alias/)).toBeInTheDocument();
    expect(
      screen.getByText(/Ikke konfigureret/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Ingen mail-drop-bilag indlæst endnu/),
    ).toBeInTheDocument();
  });

  test("konfigureret status viser host + mailbox", async () => {
    renderView(
      payload({
        imapConfigured: true,
        imapStatus: {
          host: "imap.example.com",
          port: 993,
          secure: true,
          username: "rentemester@example.com",
          mailbox: "INBOX",
        },
        mailAlias: "acme-aps",
      }),
    );
    expect(await screen.findByText(/Konfigureret/)).toBeInTheDocument();
    expect(
      screen.getByDisplayValue("imap.example.com"),
    ).toBeInTheDocument();
    expect(
      screen.getByDisplayValue("rentemester@example.com"),
    ).toBeInTheDocument();
    expect(screen.getByDisplayValue("acme-aps")).toBeInTheDocument();
  });

  test("inbox-rækker listes med kilde + afsender", async () => {
    renderView(
      payload({
        inbox: [
          {
            id: 42,
            documentNo: "B-42",
            source: "mail",
            uploadDatetime: "2026-05-20T10:00:00Z",
            senderName: "Energinet",
            invoiceDate: "2026-05-15",
            amountIncVat: 1234.56,
            retainUntil: "2031-12-31",
          },
        ],
      }),
    );
    expect(await screen.findByText("#42")).toBeInTheDocument();
    expect(screen.getByText("B-42")).toBeInTheDocument();
    expect(screen.getByText("Energinet")).toBeInTheDocument();
    expect(screen.getByText("mail")).toBeInTheDocument();
  });
});
