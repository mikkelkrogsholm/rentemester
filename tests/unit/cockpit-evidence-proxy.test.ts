import { describe, expect, test } from "bun:test";
import {
  internalAppIpv4,
  startLoopbackProxy,
} from "../../scripts/release/cockpit-evidence-proxy";

describe("cockpit evidence proxy", () => {
  test("accepts only one un-published internal network address", () => {
    expect(
      internalAppIpv4(
        { Networks: { evidence: { IPAddress: "172.20.0.2" } }, Ports: {} },
        "evidence",
      ),
    ).toBe("172.20.0.2");
    expect(() =>
      internalAppIpv4(
        { Networks: { evidence: { IPAddress: "8.8.8.8" } }, Ports: {} },
        "evidence",
      ),
    ).toThrow("invalid internal application IPv4");
    expect(() =>
      internalAppIpv4(
        {
          Networks: { evidence: { IPAddress: "172.20.0.2" }, bridge: { IPAddress: "172.17.0.2" } },
          Ports: {},
        },
        "evidence",
      ),
    ).toThrow("only to the internal evidence network");
    expect(() =>
      internalAppIpv4(
        { Networks: { evidence: { IPAddress: "172.20.0.2" } }, Ports: { "4319/tcp": [] } },
        "evidence",
      ),
    ).toThrow("must not publish container ports");
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
});
