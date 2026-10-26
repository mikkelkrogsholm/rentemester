/** Small, transport-safe description of what a reported figure is based on. */
export const DATA_COVERAGE_LABELS = {
  current: "Aktuel bogføring",
  historical: "Historisk kilde",
  scenario: "Scenarie",
  incomplete: "Ufuldstændigt grundlag",
  final: "Endelig/låst",
} as const;

export type DataCoverageKind = keyof typeof DATA_COVERAGE_LABELS;
export type DataCoverage = {
  kind: DataCoverageKind;
  label: (typeof DATA_COVERAGE_LABELS)[DataCoverageKind];
  asOfDate: string | null;
  comparison: "available" | "not_comparable";
  provenance: "native" | "imported" | "archived" | "scenario";
  details: string[];
};

export function dataCoverage(kind: DataCoverageKind, asOfDate: string | null, comparison: DataCoverage["comparison"], provenance: DataCoverage["provenance"], details: string[] = []): DataCoverage {
  return { kind, label: DATA_COVERAGE_LABELS[kind], asOfDate, comparison, provenance, details };
}
