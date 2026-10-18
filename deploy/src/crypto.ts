import type { EncryptionKey, SessionToken, TokenHash } from "./types.ts";

const IV_BYTES = 12;
const GCM_TAG_BYTES = 16;
const RANDOM_TOKEN_BYTES = 32;
const ENCRYPTION_KEY_BYTES = 32;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function randomBytes(length: number): Uint8Array<ArrayBuffer> {
  return crypto.getRandomValues(new Uint8Array(length));
}

export function encodeBase64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

/** Accepts the standard or the URL-safe alphabet, padded or not. */
export function decodeBase64(text: string): Uint8Array<ArrayBuffer> | null {
  if (!/^[A-Za-z0-9+/_-]*={0,2}$/.test(text)) {
    return null;
  }
  const normalized = text.replaceAll("-", "+").replaceAll("_", "/").replace(/=+$/, "");
  if (normalized.length % 4 === 1) {
    return null;
  }
  const binary = atob(normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "="));
  return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}

export function decodeBase64Json(text: string): unknown {
  const bytes = decodeBase64(text);
  if (bytes === null) {
    return null;
  }
  try {
    return JSON.parse(decoder.decode(bytes));
  } catch {
    return null;
  }
}

async function sha256(text: string): Promise<Uint8Array<ArrayBuffer>> {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", encoder.encode(text)));
}

function randomToken(): string {
  return encodeBase64Url(randomBytes(RANDOM_TOKEN_BYTES));
}

export function newSessionToken(): SessionToken {
  return randomToken() as SessionToken;
}

/** The `state` value that ties a callback to the sign-in that started it. */
export function newOauthState(): string {
  return randomToken();
}

/** Anything that is not 32 unpadded base64url bytes cannot be a token we issued. */
export function parseSessionToken(value: string): SessionToken | null {
  return /^[A-Za-z0-9_-]{43}$/.test(value) ? (value as SessionToken) : null;
}

export async function hashSessionToken(token: SessionToken): Promise<TokenHash> {
  const digest = await sha256(token);
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("") as TokenHash;
}

/**
 * Compares two secrets without leaking where they differ. Hashing first gives both sides the
 * same length. `crypto.subtle.timingSafeEqual` is Workers only, and the tests run on Node.
 */
export async function constantTimeEqual(a: string, b: string): Promise<boolean> {
  const [digestA, digestB] = await Promise.all([sha256(a), sha256(b)]);
  let difference = 0;
  for (let index = 0; index < digestA.length; index += 1) {
    difference |= (digestA[index] ?? 0) ^ (digestB[index] ?? 0);
  }
  return difference === 0;
}

export function parseEncryptionKey(text: string): EncryptionKey | null {
  const bytes = decodeBase64(text.trim());
  return bytes?.length === ENCRYPTION_KEY_BYTES ? (bytes as EncryptionKey) : null;
}

function importKey(key: EncryptionKey): Promise<CryptoKey> {
  return crypto.subtle.importKey("raw", key, "AES-GCM", false, ["encrypt", "decrypt"]);
}

/** Output is base64url of a random 12 byte IV followed by the ciphertext and GCM tag. */
export async function encryptToken(key: EncryptionKey, plaintext: string): Promise<string> {
  const iv = randomBytes(IV_BYTES);
  const ciphertext = new Uint8Array(
    await crypto.subtle.encrypt(
      { name: "AES-GCM", iv },
      await importKey(key),
      encoder.encode(plaintext),
    ),
  );
  const sealed = new Uint8Array(IV_BYTES + ciphertext.length);
  sealed.set(iv);
  sealed.set(ciphertext, IV_BYTES);
  return encodeBase64Url(sealed);
}

/** Returns null when the value is malformed, tampered with, or sealed under another key. */
export async function decryptToken(key: EncryptionKey, sealed: string): Promise<string | null> {
  const bytes = decodeBase64(sealed);
  if (bytes === null || bytes.length < IV_BYTES + GCM_TAG_BYTES) {
    return null;
  }
  try {
    const plaintext = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: bytes.subarray(0, IV_BYTES) },
      await importKey(key),
      bytes.subarray(IV_BYTES),
    );
    return decoder.decode(plaintext);
  } catch {
    return null;
  }
}
