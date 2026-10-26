/** Shared compiler choices for production, dev, and Bun component tests. */
export const stylexConfig = {
  importSources: ['@stylexjs/stylex'],
  useCSSLayers: { prefix: 'stylex' },
  unstable_moduleResolution: { type: 'commonJS' as const, rootDir: import.meta.dir },
  runtimeInjection: false,
  styleResolution: 'application-order' as const,
  propertyValidationMode: 'throw' as const,
  // StyleX 0.19's media-query-order parser rejects EOF tokens under Bun.
  // Explicit mobile overrides preserve the intended responsive cascade.
  enableMediaQueryOrder: false,
  // Retain token module side effects after Babel inlines variable references.
  treeshakeCompensation: true,
};
