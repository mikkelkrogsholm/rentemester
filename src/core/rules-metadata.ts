import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export type RuleBundleMetadata = {
  name: string;
  path: string;
  version: string;
  ruleIds: string[];
  sourceIds: string[];
  declaredSources: string[];
  vatCodes: string[];
};

const rulesDir = fileURLToPath(new URL("../../rules/dk/", import.meta.url));
const legalSourcesPath = fileURLToPath(new URL("../../sources/legal-sources.json", import.meta.url));

function extractVersion(text: string, file: string) {
  const match = text.match(/^version:\s*(\S+)$/m);
  if (!match) throw new Error(`Missing version in ${file}`);
  return match[1];
}

function extractSectionValues(text: string, section: string, itemPattern: RegExp) {
  const values: string[] = [];
  let inSection = false;
  for (const line of text.split(/\r?\n/)) {
    if (/^[A-Za-z_]+:/.test(line)) {
      inSection = line.trim() === `${section}:`;
      continue;
    }
    if (!inSection) continue;
    const match = line.match(itemPattern);
    if (match) values.push(match[1]);
  }
  return values;
}

export function readRuleBundleMetadata(): RuleBundleMetadata[] {
  return readdirSync(rulesDir)
    .filter((file) => file.endsWith(".yaml"))
    .sort()
    .map((file) => {
      const path = join(rulesDir, file);
      const text = readFileSync(path, "utf8");
      return {
        name: file.replace(/\.yaml$/, ""),
        path,
        version: extractVersion(text, file),
        ruleIds: [...text.matchAll(/^\s*-\s*rule_id:\s*(\S+)$/gm)].map((match) => match[1]),
        sourceIds: [...text.matchAll(/^\s*source_id:\s*(\S+)$/gm)].map((match) => match[1]),
        declaredSources: extractSectionValues(text, "sources", /^\s*-\s*(DK-[A-Z0-9-]+)$/),
        vatCodes: extractSectionValues(text, "vat_codes", /^\s*-\s*code:\s*(\S+)$/),
      };
    });
}

export function currentRuleBundleVersion() {
  return readRuleBundleMetadata()
    .map((bundle) => `${bundle.name}=${bundle.version}`)
    .join(";");
}

export function readLegalSourceIds() {
  const sources = JSON.parse(readFileSync(legalSourcesPath, "utf8")) as Array<{ id: string }>;
  return sources.map((source) => source.id);
}

export type LegalSource = {
  id: string;
  title: string;
  authority: string;
  category: string;
  url: string;
  xmlUrl?: string;
  notes?: string;
};

/**
 * Reads the full legal-source catalog used as citation targets. Used by the
 * Lovgrundlag-viewer (#347) so the cockpit can link each rule's citation to
 * the authoritative retsinformation.dk URL.
 */
export function readLegalSources(): LegalSource[] {
  return JSON.parse(readFileSync(legalSourcesPath, "utf8")) as LegalSource[];
}

export type RuleProvisionCitation = {
  ref: string;
  textHash: string;
};

export type RuleMetadata = {
  ruleId: string;
  sourceId: string;
  bundle: string;
  name: string;
  explanation: string;
  provisions: RuleProvisionCitation[];
  severity: string;
  category: string;
};

function stripQuotes(value: string): string {
  const trimmed = value.trim();
  if (
    trimmed.length >= 2 &&
    ((trimmed.startsWith('"') && trimmed.endsWith('"')) ||
      (trimmed.startsWith("'") && trimmed.endsWith("'")))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function indentOf(line: string): number {
  let count = 0;
  while (count < line.length && line[count] === " ") count += 1;
  return count;
}

// A `>`-style folded block scalar opens a continuation block. A `|`-style
// literal block scalar would need newline-preserving joins the review reader
// does not implement, so it is rejected loudly rather than silently folded.
function opensBlockScalar(
  indicator: string,
  field: string,
  bundle: string,
  ruleId: string,
): boolean {
  if (/^>[+-]?$/.test(indicator)) return true;
  if (/^\|[+-]?$/.test(indicator)) {
    throw new Error(
      `${bundle}.yaml: rule ${ruleId} — literal block scalars (\`|\`) are not ` +
        `supported for ${field}; use \`>-\``,
    );
  }
  return false;
}

// Per-rule reader for the regulatory-coverage engine. The repo hand-parses
// YAML; this follows that pattern, keyed purely on indentation. A malformed
// citation throws rather than being silently dropped — a dropped citation would
// make coverage look better than it really is.
export function parseRuleBundle(text: string, bundle: string): RuleMetadata[] {
  const rules: RuleMetadata[] = [];
  const lines = text.split(/\r?\n/);

  let current: {
    ruleId: string;
    sourceId: string;
    name: string;
    explanation: string;
    severity: string;
    category: string;
    provisions: RuleProvisionCitation[];
  } | null = null;
  let inProvisions = false;
  let inMachineRule = false;
  let pendingRef: string | null = null;
  let pendingHash: string | null = null;
  // A YAML folded/literal block scalar in progress (`name: >-` etc.). Its
  // continuation lines are indented deeper than the key.
  let pendingBlock: { field: "name" | "explanation"; keyIndent: number; lines: string[] }
    | null = null;

  const flushBlock = () => {
    if (!pendingBlock || !current) {
      pendingBlock = null;
      return;
    }
    // Folded (`>`) scalars join on single spaces. Literal (`|`) blocks are
    // rejected upstream; blank lines inside a folded block are not preserved —
    // fine for the single-paragraph review text the rule files use.
    const joined = pendingBlock.lines.map((l) => l.trim()).join(" ").trim();
    if (pendingBlock.field === "name") current.name = joined;
    else current.explanation = joined;
    pendingBlock = null;
  };

  const flushCitation = () => {
    if (pendingRef !== null || pendingHash !== null) {
      if (current === null || pendingRef === null || pendingHash === null) {
        throw new Error(
          `${bundle}.yaml: malformed provisions entry in rule ${current?.ruleId ?? "?"} — ` +
            `every entry needs both ref and text_hash (ref=${pendingRef}, text_hash=${pendingHash})`,
        );
      }
      current.provisions.push({ ref: pendingRef, textHash: pendingHash });
    }
    pendingRef = null;
    pendingHash = null;
  };

  const flushRule = () => {
    flushBlock();
    flushCitation();
    if (current) {
      current.provisions.sort((a, b) => (a.ref < b.ref ? -1 : a.ref > b.ref ? 1 : 0));
      rules.push({
        ruleId: current.ruleId,
        sourceId: current.sourceId,
        bundle,
        name: current.name,
        explanation: current.explanation,
        provisions: current.provisions,
        severity: current.severity,
        category: current.category,
      });
    }
    current = null;
    inProvisions = false;
    inMachineRule = false;
  };

  for (const line of lines) {
    if (line.trim().length === 0) continue;
    const leading = line.slice(0, line.length - line.trimStart().length);
    if (leading.includes("\t")) {
      throw new Error(`${bundle}.yaml: tab indentation is not allowed in rule files`);
    }
    const indent = indentOf(line);

    // While a block scalar is open, lines indented deeper than its key are
    // continuation text; the first line at or above the key indent ends it.
    if (pendingBlock) {
      if (indent > pendingBlock.keyIndent) {
        pendingBlock.lines.push(line);
        continue;
      }
      flushBlock();
    }

    if (/^\s*#/.test(line)) continue;

    // The machine_rule subtree is not needed for coverage; skip its deeper-
    // indented lines until the next rule-level (<=4-space) key.
    if (inMachineRule) {
      if (indent >= 6) continue;
      inMachineRule = false;
    }

    const ruleStart = line.match(/^\s*-\s*rule_id:\s*(\S+)\s*$/);
    if (ruleStart) {
      flushRule();
      current = {
        ruleId: ruleStart[1],
        sourceId: "",
        name: "",
        explanation: "",
        severity: "",
        category: "",
        provisions: [],
      };
      continue;
    }
    if (!current) continue;

    // A new 4-space rule-level key closes any open provisions block.
    if (indent <= 4 && inProvisions && !/^\s*-\s/.test(line)) {
      flushCitation();
      inProvisions = false;
    }

    if (inProvisions) {
      const itemStart = line.match(/^\s{6}-\s*(.*)$/);
      if (itemStart) {
        flushCitation();
        const inline = itemStart[1].match(/^ref:\s*(.+)$/);
        if (!inline) {
          throw new Error(
            `${bundle}.yaml: rule ${current.ruleId} provisions entry must be ` +
              `\`- ref: "..."\` — got: ${line.trim()}`,
          );
        }
        pendingRef = stripQuotes(inline[1]);
        continue;
      }
      if (indent >= 8) {
        if (/^\s*#/.test(line)) continue;
        const hashMatch = line.match(/^\s*text_hash:\s*(.+)$/);
        if (!hashMatch) {
          throw new Error(
            `${bundle}.yaml: rule ${current.ruleId} provisions entry has an ` +
              `unexpected line — expected \`text_hash: "..."\`, got: ${line.trim()}`,
          );
        }
        if (pendingHash !== null) {
          throw new Error(
            `${bundle}.yaml: rule ${current.ruleId} provisions entry has a duplicate text_hash`,
          );
        }
        pendingHash = stripQuotes(hashMatch[1]);
        continue;
      }
    }

    const sourceMatch = line.match(/^\s{4}source_id:\s*(\S+)\s*$/);
    if (sourceMatch) {
      current.sourceId = sourceMatch[1];
      continue;
    }
    const nameMatch = line.match(/^\s{4}name:\s*(.*?)\s*$/);
    if (nameMatch) {
      if (opensBlockScalar(nameMatch[1], "name", bundle, current.ruleId)) {
        pendingBlock = { field: "name", keyIndent: indent, lines: [] };
      } else {
        current.name = stripQuotes(nameMatch[1]);
      }
      continue;
    }
    const explanationMatch = line.match(/^\s{4}explanation:\s*(.*?)\s*$/);
    if (explanationMatch) {
      if (opensBlockScalar(explanationMatch[1], "explanation", bundle, current.ruleId)) {
        pendingBlock = { field: "explanation", keyIndent: indent, lines: [] };
      } else {
        current.explanation = stripQuotes(explanationMatch[1]);
      }
      continue;
    }
    const severityMatch = line.match(/^\s{4}severity:\s*(\S+)\s*$/);
    if (severityMatch) {
      current.severity = stripQuotes(severityMatch[1]);
      continue;
    }
    const categoryMatch = line.match(/^\s{4}category:\s*(\S+)\s*$/);
    if (categoryMatch) {
      current.category = stripQuotes(categoryMatch[1]);
      continue;
    }
    if (/^\s{4}provisions:\s*$/.test(line)) {
      inProvisions = true;
      continue;
    }
    if (/^\s{4}machine_rule:\s*$/.test(line)) {
      inMachineRule = true;
      continue;
    }
    // A deeper-indented line consumed by nothing above is most likely a
    // multi-line plain scalar; that is unsupported, so fail loud rather than
    // silently dropping the text.
    if (indent >= 6) {
      throw new Error(
        `${bundle}.yaml: rule ${current.ruleId} has an unexpected indented line ` +
          `(multi-line plain scalars are not supported — use \`>-\`): ${line.trim()}`,
      );
    }
  }
  flushRule();
  return rules;
}

export function readRuleMetadata(): RuleMetadata[] {
  const rules: RuleMetadata[] = [];
  for (const file of readdirSync(rulesDir).filter((f) => f.endsWith(".yaml")).sort()) {
    const bundle = file.replace(/\.yaml$/, "");
    rules.push(...parseRuleBundle(readFileSync(join(rulesDir, file), "utf8"), bundle));
  }
  rules.sort((a, b) => (a.ruleId < b.ruleId ? -1 : a.ruleId > b.ruleId ? 1 : 0));
  return rules;
}
