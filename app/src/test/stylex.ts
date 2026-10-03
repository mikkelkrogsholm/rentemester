import { plugin } from 'bun';
import { transformAsync } from '@babel/core';
import stylex from '@stylexjs/babel-plugin';
import syntaxTypescript from '@babel/plugin-syntax-typescript';
import { stylexConfig } from '../../stylex.config';

// Happy DOM tests assert behavior. Actual CSS is verified in compiled-browser tests.
plugin({
  name: 'rentemester-stylex-tests',
  setup(build) {
    build.onLoad({ filter: /\/app\/src\/.*\.tsx?$/ }, async ({ path }) => {
      const source = await Bun.file(path).text();
      if (!source.includes('@stylexjs/stylex')) return { contents: source, loader: path.endsWith('.tsx') ? 'tsx' : 'ts' };
      const result = await transformAsync(source, {
        filename: path, babelrc: false, configFile: false,
        plugins: [[syntaxTypescript, { isTSX: path.endsWith('.tsx') }], [stylex, { ...stylexConfig, test: true }]],
      });
      if (!result?.code) throw new Error(`StyleX test transform failed: ${path}`);
      return { contents: result.code, loader: path.endsWith('.tsx') ? 'tsx' : 'ts' };
    });
  },
});
