import { describe, expect, test, vi } from "bun:test";
import { api, ApiError } from "../api";
import type { InvoiceIssueInput } from "./invoices";
import { stubGlobal } from "../../test/globals";

const invoice: InvoiceIssueInput = { issueDate: "2026-05-16", lines: [{ description: "Synthetic service", quantity: 1, unitPriceExVat: 80 }] };
const period = { periodStart: "2026-05-01", periodEnd: "2026-05-31" };
const calls = [
  { name: "invoice preview", invoke: () => api.previewInvoice("synthetic", invoice), write: false },
  { name: "accountant export", invoke: () => api.accountantExport("synthetic", period), write: true },
];

describe("binary API transport", () => {
  for (const call of calls) {
    test(`${call.name} expires the hosted session for both JSON and malformed 401 responses`, async () => {
      const expired = vi.fn();
      window.addEventListener("rentemester:auth-expired", expired);
      try {
        for (const body of [JSON.stringify({ ok: false, code: "unauthorized", errors: ["Session expired"] }), "login page"]) {
          stubGlobal("fetch", vi.fn(async () => new Response(body, { status: 401 })));
          await expect(call.invoke()).rejects.toMatchObject({ code: "unauthorized", status: 401 });
        }
        expect(expired).toHaveBeenCalledTimes(2);
      } finally { window.removeEventListener("rentemester:auth-expired", expired); }
    });

    test(`${call.name} retains validation/conflict messages and HTTP fallbacks`, async () => {
      for (const [body, status, code, message] of [
        [JSON.stringify({ ok: false, code: "bad_request", errors: ["Invalid period"] }), 400, "bad_request", "Invalid period"],
        [JSON.stringify({ ok: false, code: "conflict", errors: ["Backup locked"] }), 409, "conflict", "Backup locked"],
        ["unavailable", 503, "internal", "HTTP 503"],
      ] as const) {
        stubGlobal("fetch", vi.fn(async () => new Response(body, { status })));
        await expect(call.invoke()).rejects.toMatchObject({ code, status, message });
      }
    });

    test(`${call.name} normalizes network and interrupted body failures without retries`, async () => {
      const fetchMock = vi.fn(async () => { throw new TypeError("offline"); });
      stubGlobal("fetch", fetchMock);
      await expect(call.invoke()).rejects.toBeInstanceOf(ApiError);
      expect(fetchMock).toHaveBeenCalledTimes(1);
      await expect(call.invoke()).rejects.toMatchObject({
        code: "network", status: 0,
        message: expect.stringContaining(call.write ? "kan være gennemført" : "prøv igen"),
      });
      const broken = new Response("partial body");
      broken.blob = async () => { throw new TypeError("stream interrupted"); };
      stubGlobal("fetch", vi.fn(async () => broken));
      await expect(call.invoke()).rejects.toMatchObject({ code: "internal", status: 200 });
    });
  }

  test("returns the PDF bytes and keeps preview a read-only POST", async () => {
    const fetchMock = vi.fn(async (_path: RequestInfo | URL, _options?: RequestInit) => new Response("synthetic PDF", { headers: { "content-type": "application/pdf" } }));
    stubGlobal("fetch", fetchMock);
    expect(await (await api.previewInvoice("synthetic / company", invoice)).text()).toBe("synthetic PDF");
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]?.[0]).toBe("/api/companies/synthetic%20%2F%20company/invoices/preview");
    const options = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(options).toMatchObject({ method: "POST", credentials: "same-origin" });
    expect(options).not.toHaveProperty("readOnly");
    expect(JSON.parse(String(options.body))).toEqual(invoice);
  });

  test("retains tar bytes, filename/count headers and explicit confirmation", async () => {
    const fetchMock = vi.fn(async (_path: RequestInfo | URL, _options?: RequestInit) => new Response("synthetic TAR", { headers: {
      "content-disposition": "attachment; filename*=UTF-8''revisor%20synthetic.tar",
      "x-rentemester-journal-entries": "3", "x-rentemester-documents": "4", "x-rentemester-bank-transactions": "5",
    } }));
    stubGlobal("fetch", fetchMock);
    const result = await api.accountantExport("synthetic", period);
    expect(await result.blob.text()).toBe("synthetic TAR");
    expect(result).toMatchObject({ filename: "revisor synthetic.tar", journalEntryCount: 3, documentCount: 4, bankTransactionCount: 5 });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({ ...period, confirm: true });
    stubGlobal("fetch", vi.fn(async () => new Response("TAR")));
    expect(await api.accountantExport("synthetic", period)).toMatchObject({ filename: "revisor-eksport-synthetic-2026-05-01-2026-05-31.tar", journalEntryCount: 0, documentCount: 0, bankTransactionCount: 0 });
  });
});

describe("invoice request serialization", () => {
  for (const input of [
    { ...invoice, vatRatePercent: 0, customerId: 5, invoiceNumber: "SYN-1", dueDate: "2026-05-31", currency: "DKK", seller: {}, buyer: { name: "Synthetic buyer" } },
    { ...invoice, customerId: 0, invoiceNumber: "", dueDate: "", currency: "" },
  ]) {
    test(`preview and issue send the same optional invoice fields (${input.customerId})`, async () => {
      const bodies: unknown[] = [];
      stubGlobal("fetch", vi.fn(async (path: RequestInfo | URL, init?: RequestInit) => {
        bodies.push(JSON.parse(String(init?.body)));
        return String(path).endsWith("preview") ? new Response("PDF") : Response.json({ ok: true, invoice: {} });
      }));
      await api.previewInvoice("synthetic", input);
      await api.issueInvoice("synthetic", input);
      const expected = input.customerId ? input : invoice;
      expect(bodies).toEqual([expected, expected]);
    });
  }
});
