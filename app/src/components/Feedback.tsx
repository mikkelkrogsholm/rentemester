// Compatibility adapters for older views. New pages use PageState directly.
import { PageState } from "./CockpitPrimitives";

export function Loading({ label = "Indlæser…" }: { label?: string }) {
  return <PageState kind="loading" title={label} />;
}

export function ErrorState({
  message,
  onRetry,
}: {
  message: string;
  onRetry?: () => void;
}) {
  return (
    <PageState kind="error" title="Siden kunne ikke hentes" onRetry={onRetry}>{message}</PageState>
  );
}

export function Banner({
  kind,
  children,
}: {
  kind: "error" | "success" | "warning";
  children: React.ReactNode;
}) {
  return (
    <div className={`banner ${kind}`} role={kind === "error" ? "alert" : "status"}>
      {children}
    </div>
  );
}
