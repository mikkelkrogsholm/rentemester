import { describe, expect, test } from "bun:test";
import {
  internalAppIpv4,
  startLoopbackProxy,
} from "../../scripts/release/cockpit-evidence-proxy";

describe("cockpit evidence proxy", () => {
  const Networks = { evidence: { IPAddress: "172.20.0.2" } };
  const HostConfig = { PortBindings: {}, PublishAllPorts: false };

  test("accepts exposed ports without host bindings", () => {
    for (const settings of [
      { Networks }, { Networks, Ports: null }, { Networks, Ports: {} },
      { Networks, Ports: { "4319/tcp": null } },
    ]) {
      for (const PortBindings of [{}, null]) {
        expect(internalAppIpv4({
          NetworkSettings: settings, HostConfig: { ...HostConfig, PortBindings },
        }, "evidence")).toBe("172.20.0.2");
      }
    }
  });

  test("rejects published or malformed runtime ports", () => {
    for (const Ports of [
      { "4319/tcp": [{ HostIp: "127.0.0.1", HostPort: "4319" }] },
      { "4319/tcp": [], "80/tcp": null }, { "bad": null }, { "65536/tcp": null }, [], false,
    ]) {
      expect(() => internalAppIpv4({ NetworkSettings: { Networks, Ports }, HostConfig }, "evidence"))
        .toThrow("must not publish container ports");
    }
  });

  test("requires explicit evidence against host port publication", () => {
    for (const host of [
      undefined, {}, { PublishAllPorts: false }, { PortBindings: {} },
      { ...HostConfig, PublishAllPorts: true }, { ...HostConfig, PublishAllPorts: "false" },
      { ...HostConfig, PortBindings: { "4319/tcp": [{ HostPort: "8080" }] } },
      { ...HostConfig, PortBindings: { "4319/tcp": null } }, { ...HostConfig, PortBindings: [] },
    ]) {
      expect(() => internalAppIpv4({ NetworkSettings: { Networks }, HostConfig: host }, "evidence"))
        .toThrow("must not publish container ports");
    }
  });

  test("requires a private address on only the internal network", () => {
    expect(() => internalAppIpv4({
      NetworkSettings: { Networks: { evidence: { IPAddress: "8.8.8.8" } } }, HostConfig,
    }, "evidence")).toThrow("invalid internal application IPv4");
    expect(() => internalAppIpv4({
      NetworkSettings: { Networks: { ...Networks, bridge: { IPAddress: "172.17.0.2" } } }, HostConfig,
    }, "evidence")).toThrow("only to the internal evidence network");
  });

  test("binds loopback and forwards method, query, headers, body, and response", async () => {
    const upstream = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      async fetch(request) {
        const url = new URL(request.url);
        return new Response(await request.text(), {
          status: 201,
          headers: {
            "content-type": "application/vnd.rentemester+json",
            "x-method": request.method,
            "x-query": url.search,
            "x-evidence-header": request.headers.get("x-evidence-header") ?? "",
          },
        });
      },
    });
    if (!upstream.port) throw new Error("test upstream did not select a port");
    const { proxy, base } = startLoopbackProxy("127.0.0.1", upstream.port);
    try {
      expect(proxy.hostname).toBe("127.0.0.1");
      const response = await fetch(`${base}/api/evidence?scenario=proxy`, {
        method: "POST",
        headers: { "content-type": "application/json", "x-evidence-header": "kept" },
        body: '{"fixture":true}',
      });
      expect(response.status).toBe(201);
      expect(response.headers.get("content-type")).toBe("application/vnd.rentemester+json");
      expect(response.headers.get("x-method")).toBe("POST");
      expect(response.headers.get("x-query")).toBe("?scenario=proxy");
      expect(response.headers.get("x-evidence-header")).toBe("kept");
      expect(await response.text()).toBe('{"fixture":true}');
    } finally {
      await proxy.stop(true);
      await upstream.stop(true);
    }
  });

  test("returns a deterministic 502 when the upstream loopback port is closed", async () => {
    const listener = Bun.listen({
      hostname: "127.0.0.1",
      port: 0,
      socket: { data() {}, open() {} },
    });
    const closedPort = listener.port;
    listener.stop();
    const { proxy, base } = startLoopbackProxy("127.0.0.1", closedPort);
    try {
      for (let attempt = 0; attempt < 2; attempt++) {
        const response = await fetch(`${base}/api/ready`);
        expect(response.status).toBe(502);
        expect(response.headers.get("content-type")).toBe("text/plain;charset=utf-8");
        expect(await response.text()).toBe("upstream unavailable");
      }
    } finally {
      await proxy.stop(true);
    }
  });
});
