import { describe, expect, it } from "vitest";
import { MAX_DOC_BYTES } from "./doc.ts";
import { createTestApp } from "./testing/app.ts";

async function signedInApp() {
  const app = createTestApp();
  await app.signIn();
  return app;
}

function put(app: Awaited<ReturnType<typeof signedInApp>>, body: BodyInit | null) {
  return app.request("/api/doc", { method: "PUT", body });
}

/** An envelope that is exactly `bytes` long in UTF-8. */
function envelopeOfSize(bytes: number): string {
  const frame = '{"version":5,"doc":{"pad":""}}';
  return `{"version":5,"doc":{"pad":"${"x".repeat(bytes - frame.length)}"}}`;
}

describe("GET /api/doc", () => {
  it("answers 204 when the teacher has stored nothing", async () => {
    const app = await signedInApp();
    const response = await app.request("/api/doc");
    expect(response.status).toBe(204);
    expect(await response.text()).toBe("");
  });
});

describe("PUT /api/doc", () => {
  it("stores the envelope and returns it byte for byte as application/json", async () => {
    const app = await signedInApp();
    const envelope =
      '{ "version": 5,\n  "doc": { "emoji": "😀 é \\u00e9", "n": 1.0, "big": 12345678901234567890,\n"exp": 1E2, "nested": {"b":1,"a":2} } }\n';

    expect((await put(app, envelope)).status).toBe(204);

    const response = await app.request("/api/doc");
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toBe("application/json");
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(await response.text()).toBe(envelope);
  });

  it("replaces the previous doc (last write wins)", async () => {
    const app = await signedInApp();
    await put(app, '{"version":5,"doc":{"a":1}}');
    await put(app, '{"version":5,"doc":{"b":2}}');
    expect(await (await app.request("/api/doc")).text()).toBe('{"version":5,"doc":{"b":2}}');
  });

  it("accepts a doc up to exactly 1 MiB and rejects one byte more with 413", async () => {
    const app = await signedInApp();

    const atCap = envelopeOfSize(MAX_DOC_BYTES);
    expect(new TextEncoder().encode(atCap).length).toBe(MAX_DOC_BYTES);
    expect((await put(app, atCap)).status).toBe(204);
    expect(await (await app.request("/api/doc")).text()).toBe(atCap);

    const overCap = envelopeOfSize(MAX_DOC_BYTES + 1);
    const response = await put(app, overCap);
    expect(response.status).toBe(413);
    expect(await response.json()).toEqual({ error: "too_large" });
    expect(await (await app.request("/api/doc")).text()).toBe(atCap);
  });

  it("measures bytes, not characters", async () => {
    const app = await signedInApp();
    const frame = '{"version":5,"doc":{"t":""}}';
    const multibyte = "é".repeat((MAX_DOC_BYTES - frame.length) / 2 + 1);
    const body = `{"version":5,"doc":{"t":"${multibyte}"}}`;
    expect(body.length).toBeLessThan(MAX_DOC_BYTES);
    expect((await put(app, body)).status).toBe(413);
  });

  it("rejects an oversized body that declares a small Content-Length", async () => {
    const app = await signedInApp();
    const response = await app.request("/api/doc", {
      method: "PUT",
      body: envelopeOfSize(MAX_DOC_BYTES + 10),
      headers: { "content-length": "10" },
    });
    expect(response.status).toBe(413);
  });

  it("rejects a declared Content-Length over the cap without reading the body", async () => {
    const app = await signedInApp();
    let pulled = false;
    const body = new ReadableStream<Uint8Array>(
      {
        pull(controller) {
          pulled = true;
          controller.enqueue(new TextEncoder().encode("{}"));
          controller.close();
        },
      },
      { highWaterMark: 0 },
    );
    const response = await app.request("/api/doc", {
      method: "PUT",
      body,
      duplex: "half",
      headers: { "content-length": String(MAX_DOC_BYTES + 1) },
    } as RequestInit);
    expect(response.status).toBe(413);
    expect(pulled).toBe(false);
  });

  it.each([
    ["an empty body", ""],
    ["text that is not JSON", "hello"],
    ["truncated JSON", '{"version":5,"doc":{'],
    ["a JSON array", "[1,2]"],
    ["null", "null"],
    ["a missing version", '{"doc":{}}'],
    ["a string version", '{"version":"5","doc":{}}'],
    ["a version too large for a number", '{"version":1e999,"doc":{}}'],
    ["a missing doc", '{"version":5}'],
    ["a null doc", '{"version":5,"doc":null}'],
    ["an array doc", '{"version":5,"doc":[]}'],
    ["a string doc", '{"version":5,"doc":"x"}'],
    ["a byte order mark", '﻿{"version":5,"doc":{}}'],
  ])("rejects %s with 400", async (_label, body) => {
    const app = await signedInApp();
    await put(app, '{"version":5,"doc":{"keep":true}}');

    const response = await put(app, body);

    expect(response.status).toBe(400);
    expect(await response.json()).toEqual({ error: "invalid_doc" });
    expect(await (await app.request("/api/doc")).text()).toBe('{"version":5,"doc":{"keep":true}}');
  });

  it("rejects bytes that are not UTF-8", async () => {
    const app = await signedInApp();
    const prefix = new TextEncoder().encode('{"version":5,"doc":{"t":"');
    const suffix = new TextEncoder().encode('"}}');
    const body = new Uint8Array([...prefix, 0xff, 0xfe, ...suffix]);
    expect((await put(app, body)).status).toBe(400);
  });

  it("rejects a PUT without a body", async () => {
    const app = await signedInApp();
    expect((await put(app, null)).status).toBe(400);
  });
});
