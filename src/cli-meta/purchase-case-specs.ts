import type { CommandSpec } from "./_shared";

export const purchaseCaseSpecs: CommandSpec[] = [
  { key: "purchase-case list", usage: "purchase-case list --company <path>", description: "Læs de aktuelle purchase cases uden mutation.", allowedFlags: ["--company"] },
  { key: "purchase-case show", usage: "purchase-case show --company <path> --case-id <id>", description: "Læs én current purchase case uden mutation.", allowedFlags: ["--company", "--case-id"] },
];
