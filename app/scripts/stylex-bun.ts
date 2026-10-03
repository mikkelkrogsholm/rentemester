import { stylexBunPlugin } from './stylex-plugin';
import { stylexConfig } from '../stylex.config';
// Dev-only injection allows Bun HTML hot reload to carry styles immediately.
export default stylexBunPlugin({ ...stylexConfig, dev: true, runtimeInjection: true });
