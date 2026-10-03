import { useEffect, useRef } from "react";
import type { Chart, ChartType } from "chart.js";
import { designTokens } from "../../../src/design/tokens";

/** Canvas keeps its first font rendering until Chart.js draws again. */
export function useChartFonts<T extends ChartType>() {
  const ref = useRef<Chart<T>>(null);
  useEffect(() => {
    if (!document.fonts) return;
    let active = true;
    void Promise.all([
      document.fonts.load(`${designTokens.typography.sizeXs} "${designTokens.typography.monoFamily}"`),
      document.fonts.load(`${designTokens.typography.sizeSm} "${designTokens.typography.bodyFamily}"`),
    ]).then(() => {
      if (active) ref.current?.update("none");
    }).catch(() => { /* Preserve the browser fallback if a font cannot load. */ });
    return () => { active = false; };
  }, []);
  return ref;
}
