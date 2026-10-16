import { describe, expect, it } from "vitest";
import {
  constantTimeEqual,
  decodeBase64,
  decryptToken,
  encodeBase64Url,
  encryptToken,
  hashSessionToken,
  newSessionToken,
  parseEncryptionKey,
  parseSessionToken,
  randomBytes,
} from "./crypto.ts";
import type { EncryptionKey } from "./types.ts";

const TEST_KEY = btoa(String.fromCharCode(...randomBytes(32)));

function key(text = TEST_KEY): EncryptionKey {
  const parsed = parseEncryptionKey(text);
  if (parsed === null) {
    throw new Error("test key must parse");
  }
  return parsed;
}

describe("refresh token encryption", () => {
  it("round trips a token", async () => {
    const sealed = await encryptToken(key(), "1//refresh-token-value");
    expect(sealed).not.toContain("refresh-token-value");
    expect(await decryptToken(key(), sealed)).toBe("1//refresh-token-value");
  });

  it("prefixes a fresh random 12 byte IV, so equal plaintexts seal differently", async () => {
    const first = await encryptToken(key(), "same");
    const second = await encryptToken(key(), "same");
    expect(first).not.toBe(second);
    const sealedBytes = decodeBase64(first);
    expect(sealedBytes?.length).toBe(12 + "same".length + 16);
  });

  it("rejects a tampered ciphertext", async () => {
    const sealed = decodeBase64(await encryptToken(key(), "secret"));
    if (sealed === null) {
      throw new Error("sealed token must decode");
    }
    for (const index of [0, 12, sealed.length - 1]) {
      const tampered = sealed.slice();
      tampered[index] = (tampered[index] ?? 0) ^ 1;
      expect(await decryptToken(key(), encodeBase64Url(tampered))).toBeNull();
    }
  });

  it("rejects a value sealed under another key", async () => {
    const sealed = await encryptToken(key(), "secret");
    const other = btoa(String.fromCharCode(...randomBytes(32)));
    expect(await decryptToken(key(other), sealed)).toBeNull();
  });

  it("rejects truncated and malformed input", async () => {
    expect(await decryptToken(key(), "")).toBeNull();
    expect(await decryptToken(key(), "AAAA")).toBeNull();
    expect(await decryptToken(key(), "not base64 !!")).toBeNull();
  });
});

describe("encryption key parsing", () => {
  it("accepts base64 of exactly 32 bytes in either alphabet", () => {
    expect(parseEncryptionKey(TEST_KEY)).not.toBeNull();
    expect(parseEncryptionKey(encodeBase64Url(randomBytes(32)))).not.toBeNull();
  });

  it("rejects other lengths and garbage", () => {
    expect(parseEncryptionKey(btoa("short"))).toBeNull();
    expect(parseEncryptionKey(btoa("x".repeat(33)))).toBeNull();
    expect(parseEncryptionKey("")).toBeNull();
    expect(parseEncryptionKey("%%%")).toBeNull();
  });
});

describe("session tokens", () => {
  it("are 32 random bytes in base64url and hash to SHA-256 hex", async () => {
    const token = newSessionToken();
    expect(token).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(newSessionToken()).not.toBe(token);

    const hash = await hashSessionToken(token);
    expect(hash).toMatch(/^[0-9a-f]{64}$/);
    expect(hash).not.toContain(token);
    expect(await hashSessionToken(token)).toBe(hash);
  });

  it("parse only well-formed cookie values", () => {
    expect(parseSessionToken(newSessionToken())).not.toBeNull();
    expect(parseSessionToken("")).toBeNull();
    expect(parseSessionToken("short")).toBeNull();
    expect(parseSessionToken(`${newSessionToken()}=`)).toBeNull();
    expect(parseSessionToken(`${"a".repeat(42)}+`)).toBeNull();
  });
});

describe("constant time comparison", () => {
  it("is true only for equal strings, whatever their lengths", async () => {
    expect(await constantTimeEqual("state-value", "state-value")).toBe(true);
    expect(await constantTimeEqual("state-value", "state-valuf")).toBe(false);
    expect(await constantTimeEqual("state-value", "state")).toBe(false);
    expect(await constantTimeEqual("", "x")).toBe(false);
    expect(await constantTimeEqual("", "")).toBe(true);
  });
});
