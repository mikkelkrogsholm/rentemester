import type { CommandSpec } from "./_shared";

// ===== RECURRING INVOICES (#118) =====
export const recurringInvoiceSpecs: CommandSpec[] = [
  {
    key: "recurring-invoice create",
    usage: "recurring-invoice create --company <path> --input <file.json>",
    description: "Opretter en gentagende fakturaskabelon (interval, kunde, linjer, moms, leveringsperiode).",
    allowedFlags: ["--company", "--input"],
    examplePath: "examples/recurring-invoice.create.json",
    exampleHint: "rentemester recurring-invoice create --example > skabelon.json",
    inputNotes: [
      "name: tekst",
      'interval: "monthly" | "quarterly" | "yearly"',
      "firstIssueDate: YYYY-MM-DD",
      "paymentTermsDays: heltal 0-365 (standard 30)",
      'deliveryPeriodMode: "issue_month" | "interval_window" | "none"',
      "invoice: faktura-payload (samme felter som invoice issue, uden issueDate/invoiceNumber)",
    ],
  },
  {
    key: "recurring-invoice generate",
    usage: "recurring-invoice generate --company <path> --template-id <n> --as-of <YYYY-MM-DD>",
    description: "Materialiserer deterministisk den faktura der er forfalden for skabelonen pr. --as-of (idempotent pr. periode).",
    allowedFlags: ["--company", "--template-id", "--as-of"],
  },
  {
    key: "recurring-invoice list",
    usage: "recurring-invoice list --company <path> [--include-inactive]",
    description: "Lister gentagende fakturaskabeloner.",
    allowedFlags: ["--company", "--include-inactive"],
  },
  // ===== END RECURRING INVOICES (#118) =====
];
