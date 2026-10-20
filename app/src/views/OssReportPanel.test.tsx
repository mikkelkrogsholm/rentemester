import { expect, test } from "bun:test";
import { render, screen, fireEvent } from "@testing-library/react";
import { OssReportPanel } from "./OssReportPanel";
import { stubGlobal } from "../test/globals";
test("OSS reads explicitly applied dates and signed historical DKK base", async () => {
 const calls: Array<{ path: string; init?: RequestInit }> = [];
 stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
  const path = String(input); calls.push({ path, init }); const q = new URL(path, "http://synthetic").searchParams;
  return new Response(JSON.stringify({ ok: true, periodStart: q.get("from"), periodEnd: q.get("to"), ossConsumerSalesBase: -1000, entryCount: 1, submission: false, vatReport: {}, warnings: ["Kun grundlag; ingen indberetning"], errors: [] }));
 });
 render(<OssReportPanel slug="synthetic" />);
 fireEvent.change(screen.getByLabelText("OSS fra"), { target: { value: "2025-10-12" } }); fireEvent.change(screen.getByLabelText("OSS til"), { target: { value: "2025-10-13" } }); expect(calls).toHaveLength(0);
 fireEvent.click(screen.getByRole("button", { name: "Vis OSS-rapport" })); expect(await screen.findByText("OSS-periode: 2025-10-12 – 2025-10-13")).toBeInTheDocument();
 expect(screen.getByText(/^-1.000,00/)).toBeInTheDocument(); expect(screen.getByText("1")).toBeInTheDocument(); expect(screen.getByText("Kun grundlag; ingen indberetning")).toBeInTheDocument();
 expect(calls[0]!.path).toBe("/api/companies/synthetic/oss-report?from=2025-10-12&to=2025-10-13"); expect(calls[0]!.init?.method).toBeUndefined(); expect(calls[0]!.init?.body).toBeUndefined();
 fireEvent.change(screen.getByLabelText("OSS til"), { target: { value: "2025-10-14" } }); expect(calls).toHaveLength(1); expect(screen.getByText("OSS-periode: 2025-10-12 – 2025-10-13")).toBeInTheDocument();
});
test("rejected read hides previous amounts", async () => {
 let reject = false;
 stubGlobal("fetch", async () => new Response(JSON.stringify(reject ? { ok: false, errors: ["Adgang afvist"], code: "forbidden" } : { ok: true, periodStart: "2025-01-01", periodEnd: "2025-12-31", ossConsumerSalesBase: 1000, entryCount: 1, submission: false, vatReport: {}, warnings: [], errors: [] }), { status: reject ? 403 : 200 }));
 render(<OssReportPanel slug="synthetic" />); fireEvent.click(screen.getByRole("button", { name: "Vis OSS-rapport" })); expect(await screen.findByText(/^1.000,00/)).toBeInTheDocument(); reject = true;
 fireEvent.click(screen.getByRole("button", { name: "Vis OSS-rapport" })); expect(await screen.findByText("Adgang afvist")).toBeInTheDocument(); expect(screen.queryByText(/^1.000,00/)).not.toBeInTheDocument();
});
