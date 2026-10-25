import { expect, test, vi } from "bun:test";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NativeChartSetup } from "./NativeChartSetup";
import { api } from "../lib/api";
import type { CompanyAccounts } from "../lib/types";
import { renderAt } from "../test/render";

const data: CompanyAccounts = {
  slug: "synthetic", nativeSetup: true,
  company: { name: "Synthetic", cvr: null, country: "DK", currency: "DKK" },
  accounts: [
    { accountNo: "2000", name: "Synthetic bank", type: "asset", normalBalance: "debit", defaultVatCode: null, hasPostings: false, active: true },
    { accountNo: "2100", name: "Inactive bank", type: "asset", normalBalance: "debit", defaultVatCode: null, hasPostings: false, active: false },
    { accountNo: "2200", name: "Credit asset", type: "asset", normalBalance: "credit", defaultVatCode: null, hasPostings: false, active: true },
  ], byType: { asset: 3 },
  accountRoles: { status: "incomplete", missing: ["bank"], ambiguous: [], resolutions: [], proposals: [], candidates: [], reasons: [] },
};

test("owner identifies exact chart fields, creates an account and confirms only a compatible active bank role", async () => {
  const user = userEvent.setup();
  const create = vi.spyOn(api, "createAccount").mockResolvedValue({ accountNo: "3000" });
  const confirm = vi.spyOn(api, "confirmAccountRole").mockResolvedValue({ ok: true, role: "bank", accountNo: "2000", version: 1, confirmedBy: "user:synthetic", confirmationSource: "explicit" });
  const saved = vi.fn();
  renderAt(<NativeChartSetup slug="synthetic" data={data} onSaved={saved} />, { route: "/companies/synthetic/kontoplan" });
  for (const label of ["Kontotype", "Normal saldo", "Kontorolle", "Konto til rollen"]) {
    // Keep the visible label separate from option text: Playwright reads the full label text.
    expect(screen.getByText(label, { exact: true })).toBeInTheDocument();
    const control = screen.getByLabelText(label, { exact: true });
    expect(screen.getByRole("combobox", { name: label })).toBe(control);
    expect(Array.from((control as HTMLSelectElement).labels ?? [], caption => caption.textContent?.trim())).toEqual([label]);
  }
  await user.type(screen.getByLabelText("Kontonummer", { exact: true }), "3000");
  await user.type(screen.getByLabelText("Kontonavn", { exact: true }), "Synthetic equity");
  await user.selectOptions(screen.getByLabelText("Kontotype", { exact: true }), "equity");
  await user.selectOptions(screen.getByLabelText("Normal saldo", { exact: true }), "credit");
  await user.click(screen.getByRole("button", { name: "Opret konto" }));
  expect(create).toHaveBeenCalledTimes(1);
  expect(create).toHaveBeenCalledWith("synthetic", { accountNo: "3000", name: "Synthetic equity", type: "equity", normalBalance: "credit", defaultVatCode: null });
  await user.selectOptions(screen.getByLabelText("Kontorolle", { exact: true }), "bank");
  const accounts = screen.getByLabelText("Konto til rollen", { exact: true });
  expect(within(accounts).getByRole("option", { name: "2000 — Synthetic bank" })).toBeInTheDocument();
  expect(within(accounts).queryByRole("option", { name: /Inactive bank|Credit asset/ })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Bekræft kontorolle" })).toBeDisabled();
  await user.selectOptions(accounts, "2000");
  await user.click(screen.getByRole("button", { name: "Bekræft kontorolle" }));
  expect(confirm).toHaveBeenCalledTimes(1);
  expect(confirm).toHaveBeenCalledWith("synthetic", "bank", "2000");
  expect(saved).toHaveBeenCalledTimes(2);
});
