declare const brand: unique symbol;
type Brand<T, Name extends string> = T & { readonly [brand]: Name };

/** Google's stable account identifier (`sub`), the primary key of a teacher. */
export type Sub = Brand<string, "Sub">;
/** The raw random session token. It only ever lives in the cookie. */
export type SessionToken = Brand<string, "SessionToken">;
/** SHA-256 hex of a session token. This is what the database stores. */
export type TokenHash = Brand<string, "TokenHash">;
/** A 32 byte AES-256 key, decoded from the `TOKEN_ENC_KEY` secret. */
export type EncryptionKey = Brand<Uint8Array<ArrayBuffer>, "EncryptionKey">;

export function toSub(value: string): Sub | null {
  return value === "" ? null : (value as Sub);
}

export interface Teacher {
  readonly sub: Sub;
  readonly email: string;
  readonly name: string;
}

/**
 * Outcome of a call to Google. `reauthorize` means the teacher has to sign in again. Everything
 * else that can go wrong upstream is `unavailable`.
 */
export type Fetched<T> =
  | { readonly kind: "ok"; readonly value: T }
  | { readonly kind: "reauthorize" }
  | { readonly kind: "unavailable" };
