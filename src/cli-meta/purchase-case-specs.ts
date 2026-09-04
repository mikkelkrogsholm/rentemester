import type { CommandSpec } from "./_shared";

export const purchaseCaseSpecs: CommandSpec[] = [
  { key: "purchase-case list", usage: "purchase-case list --company <path>", description: "Læs de aktuelle purchase cases uden mutation.", allowedFlags: ["--company"] },
  { key: "purchase-case show", usage: "purchase-case show --company <path> --case-id <id>", description: "Læs én current purchase case uden mutation.", allowedFlags: ["--company", "--case-id"] },
  { key: "purchase-case create", usage: "purchase-case create --company <path> --source-kind <document|bank_transaction|payable> --source-id <n> --idempotency-key <key> --confirm yes --actor <actor>", description: "Opretter en kildebundet foreløbig purchase case uden ledger- eller VAT-mutation.", allowedFlags: ["--company", "--case-id", "--source-kind", "--source-id", "--documentation-outcome", "--note", "--idempotency-key", "--confirm", "--actor"] },
];
