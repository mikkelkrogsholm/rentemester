export type NetworkSettings = {
  Networks?: Record<string, { IPAddress?: unknown }>;
  Ports?: unknown;
};

export function internalAppIpv4(
  settings: NetworkSettings,
  expectedNetwork: string,
) {
  const networks = settings.Networks;
  if (!networks || typeof networks !== "object" || Array.isArray(networks))
    throw new Error("Docker inspect did not return application networks");
  const names = Object.keys(networks);
  if (names.length !== 1 || names[0] !== expectedNetwork)
    throw new Error("candidate must be attached only to the internal evidence network");
  if (
    settings.Ports !== null &&
    (!settings.Ports ||
      typeof settings.Ports !== "object" ||
      Array.isArray(settings.Ports) ||
      Object.keys(settings.Ports).length !== 0)
  )
    throw new Error("candidate must not publish container ports");
  const address = networks[expectedNetwork]?.IPAddress;
  if (typeof address !== "string")
    throw new Error("Docker inspect did not return an internal application IPv4");
  const parts = address.split(".").map(Number);
  if (
    parts.length !== 4 ||
    parts.some((part) => !Number.isInteger(part) || part < 0 || part > 255) ||
    !(
      parts[0] === 10 ||
      (parts[0] === 172 && parts[1] >= 16 && parts[1] <= 31) ||
      (parts[0] === 192 && parts[1] === 168)
    )
  )
    throw new Error("Docker inspect returned an invalid internal application IPv4");
  return address;
}

export function startLoopbackProxy(appAddress: string, appPort = 4319) {
  if (!Number.isInteger(appPort) || appPort < 1 || appPort > 65535)
    throw new Error("evidence proxy target port must be valid");
  const appOrigin = `http://${appAddress}:${appPort}`;
  const proxy = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const incoming = new URL(request.url);
      const target = new URL(`${incoming.pathname}${incoming.search}`, appOrigin);
      const headers = new Headers(request.headers);
      // The proxy itself chooses the only allowed upstream host.
      headers.delete("host");
      headers.delete("connection");
      try {
        return await fetch(target, {
          method: request.method,
          headers,
          body: request.method === "GET" || request.method === "HEAD" ? undefined : request.body,
        });
      } catch {
        return new Response("upstream unavailable", { status: 502 });
      }
    },
  });
  const port = proxy.port;
  if (
    proxy.hostname !== "127.0.0.1" ||
    !Number.isInteger(port) ||
    port === undefined ||
    port < 1
  )
    throw new Error("evidence proxy must bind to an ephemeral loopback port");
  return { proxy, base: `http://127.0.0.1:${port}` };
}
