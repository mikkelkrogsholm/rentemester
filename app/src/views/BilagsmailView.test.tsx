import { describe, expect, test } from "bun:test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
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


test("mail GET never polls; one confirmed pass sends exact body and reloads status", async () => {
  const data=payload({imapConfigured:true});
  mockFetch({"GET /api/companies/acme-aps/bilagsmail":{...data,bilagsmail:{...data.bilagsmail,polling:{enabled:false,state:"configured"},pendingReviewCount:2}},"POST /api/companies/acme-aps/bilagsmail/poll":{}});
  renderAt(<BilagsmailView />,{route:"/companies/acme-aps/bilagsmail",path:"/companies/:slug/bilagsmail"});
  await screen.findByRole("button",{name:"Gennemgå postkasse nu"});
  const calls=()=> (globalThis.fetch as unknown as {mock:{calls:Array<[unknown,RequestInit]>}}).mock.calls.filter(c=>String(c[0]).endsWith("/poll"));
  expect(calls()).toHaveLength(0);
  expect(screen.getByRole("link",{name:"Åbn fælles dokumentindbakke"})).toHaveAttribute("href","/companies/acme-aps/workspace-inbox");
  fireEvent.click(screen.getByRole("button",{name:"Gennemgå postkasse nu"}));
  const dialog=await screen.findByRole("dialog",{name:"Bekræft mailgennemgang"});
  expect(calls()).toHaveLength(0);
  fireEvent.click(within(dialog).getByRole("button",{name:"Hent mail én gang"}));
  await waitFor(()=>expect(calls()).toHaveLength(1));
  expect(JSON.parse(String(calls()[0][1].body))).toEqual({confirm:true});
  await screen.findByText("Mailgennemgangen er afsluttet. Kontrollér status og den fælles indbakke.");
  expect(calls()).toHaveLength(1);
});

test("an uncertain poll stays blocked after a status read and never repeats automatically", async () => {
  mockFetch({"GET /api/companies/acme-aps/bilagsmail":payload({imapConfigured:true}),"POST /api/companies/acme-aps/bilagsmail/poll":{__error:{code:"internal",message:"Modtagelsens udfald er uafklaret"}}});
  renderAt(<BilagsmailView />,{route:"/companies/acme-aps/bilagsmail",path:"/companies/:slug/bilagsmail"});
  fireEvent.click(await screen.findByRole("button",{name:"Gennemgå postkasse nu"}));
  const dialog=await screen.findByRole("dialog",{name:"Bekræft mailgennemgang"});
  fireEvent.click(within(dialog).getByRole("button",{name:"Hent mail én gang"}));
  await within(dialog).findByText(/Serverens resultat kunne ikke bekræftes/);
  expect(within(dialog).getByRole("button",{name:"Hent mail én gang"})).toBeDisabled();
  fireEvent.click(within(dialog).getByRole("button",{name:"Kontrollér status"}));
  await waitFor(()=>expect(within(dialog).getByRole("button",{name:"Kontrollér status"})).not.toBeDisabled());
  const calls=(globalThis.fetch as unknown as {mock:{calls:Array<[unknown,RequestInit]>}}).mock.calls.filter(c=>String(c[0]).endsWith("/poll"));
  expect(calls).toHaveLength(1);
  expect(within(dialog).getByRole("button",{name:"Hent mail én gang"})).toBeDisabled();
});
