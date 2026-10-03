import { createStylexBunPlugin } from "@stylexjs/unplugin";
import type { PluginBuilder, BunPlugin } from "bun";

/** The adapter writes one CSS file during every transform. Serialize those
 * transforms so an older asynchronous write cannot replace the final CSS. */
export function stylexBunPlugin(options: Parameters<typeof createStylexBunPlugin>[0]) {
  const native = createStylexBunPlugin(options);
  return {
    ...native,
    async setup(build: PluginBuilder) {
      let pending = Promise.resolve<unknown>(undefined);
      const ordered = new Proxy(build, {
        get(target, name) {
          if (name === "onLoad") return ((filter, callback) => target.onLoad(filter, args => {
            const next = pending.then(() => callback(args));
            pending = next.then(() => undefined, () => undefined);
            return next;
          })) satisfies typeof build.onLoad;
          const member = Reflect.get(target, name);
          return typeof member === "function" ? member.bind(target) : member;
        },
      });
      await native.setup(ordered);
    },
  } satisfies BunPlugin;
}
