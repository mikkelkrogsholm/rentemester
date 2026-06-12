import type { CommandSpec } from "./_shared";

export const dashboardSpecs: CommandSpec[] = [
  {
    key: "dashboard",
    usage: "dashboard --company <path> --out <file.html> [--as-of <YYYY-MM-DD>] [--open]",
    description: "Genererer et statisk HTML-dashboard over virksomhedens nuværende bogføringsstatus.",
    allowedFlags: ["--company", "--out", "--as-of", "--open"],
  },
];
