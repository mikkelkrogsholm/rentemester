/** Shared compiler choices for production, dev, and Bun component tests. */
export const stylexConfig = {
  importSources: ['@stylexjs/stylex'],
  useCSSLayers: { before: ['reset', 'base', 'legacy', 'layout'], prefix: 'stylex' },
  unstable_moduleResolution: { type: 'commonJS' as const, rootDir: import.meta.dir },
  runtimeInjection: false,
  // Retain token module side effects after Babel inlines variable references.
  treeshakeCompensation: true,
};
