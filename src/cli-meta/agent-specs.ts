import type { CommandSpec } from "./_shared";

// ===== RUNTIME AGENT (#183) =====
export const agentSpecs: CommandSpec[] = [
  {
    key: "agent run",
    usage: "agent run --company <slug|path> --as-of <YYYY-MM-DD> [--inbox <dir>] [--metadata-dir <dir>] [--bank-csv <file.csv>]",
    description:
      "Kører én deterministisk bogføringsloop for virksomheden: ingest bilag → bogfør det entydige → rut det usikre til exception-køen → afstem bank → tjek moms-/regnskabsår-deadlines → udskriv en slutrapport. Agenten gætter aldrig; alt usikkert bliver en exception. Samme fixture + samme --as-of giver identisk output.",
    allowedFlags: ["--company", "--as-of", "--inbox", "--metadata-dir", "--bank-csv"],
    inputNotes: [
      "--as-of er agentens eneste 'nu' — ingen wall-clock-afhængighed",
      "--inbox: mappe med bilag (ét dokument pr. fil) med parallel <stem>.json metadata",
      "--metadata-dir: hvor metadata-JSON ligger (standard: samme som --inbox)",
      "--bank-csv: bankudtog der importeres før match/afstemning",
      "Agenten bogfører som agent:rentemester-bookkeeper og handler kun inden for guardrails",
      "--json/--format json udskriver en AgentRunReport. Felternes betydning og det fulde skema er dokumenteret i docs/runtime-agent-contract.md — læs det før du parser outputtet maskinelt.",
    ],
  },
  // ===== END RUNTIME AGENT (#183) =====
];
