import { type Sub, type Teacher, type TokenHash, toSub } from "./types.ts";

export type SqlValue = string | number | null;

/**
 * The two operations the Worker needs from a database. D1 implements it in production. The tests
 * implement it on in-memory SQLite, so the SQL below runs for real in both places.
 */
export interface Sql {
  first(query: string, params?: readonly SqlValue[]): Promise<Record<string, unknown> | null>;
  run(query: string, params?: readonly SqlValue[]): Promise<void>;
}

export function d1Sql(db: D1Database): Sql {
  return {
    first: (query, params = []) =>
      db
        .prepare(query)
        .bind(...params)
        .first<Record<string, unknown>>(),
    run: async (query, params = []) => {
      await db
        .prepare(query)
        .bind(...params)
        .run();
    },
  };
}

function text(row: Record<string, unknown>, column: string): string {
  const value = row[column];
  if (typeof value !== "string") {
    throw new Error(`column ${column} is not text`);
  }
  return value;
}

function integer(row: Record<string, unknown>, column: string): number {
  const value = row[column];
  if (typeof value !== "number") {
    throw new Error(`column ${column} is not an integer`);
  }
  return value;
}

function parseSub(row: Record<string, unknown>): Sub {
  const sub = toSub(text(row, "sub"));
  if (sub === null) {
    throw new Error("column sub is empty");
  }
  return sub;
}

/** Re-signing in refreshes the profile and token but never touches the stored doc. */
export function upsertTeacher(
  sql: Sql,
  teacher: Teacher,
  refreshTokenEnc: string,
  now: number,
): Promise<void> {
  return sql.run(
    `INSERT INTO teachers (sub, email, name, refresh_token_enc, updated_at)
     VALUES (?, ?, ?, ?, ?)
     ON CONFLICT (sub) DO UPDATE SET
       email = excluded.email,
       name = excluded.name,
       refresh_token_enc = excluded.refresh_token_enc,
       updated_at = excluded.updated_at`,
    [teacher.sub, teacher.email, teacher.name, refreshTokenEnc, now],
  );
}

export async function insertSession(
  sql: Sql,
  tokenHash: TokenHash,
  sub: Sub,
  expiresAt: number,
  now: number,
): Promise<void> {
  await sql.run("DELETE FROM sessions WHERE expires_at <= ?", [now]);
  await sql.run("INSERT INTO sessions (token_hash, sub, expires_at) VALUES (?, ?, ?)", [
    tokenHash,
    sub,
    expiresAt,
  ]);
}

export interface StoredSession {
  readonly teacher: Teacher;
  readonly expiresAt: number;
}

export async function findSession(sql: Sql, tokenHash: TokenHash): Promise<StoredSession | null> {
  const row = await sql.first(
    `SELECT t.sub, t.email, t.name, s.expires_at
     FROM sessions s JOIN teachers t ON t.sub = s.sub
     WHERE s.token_hash = ?`,
    [tokenHash],
  );
  if (row === null) {
    return null;
  }
  return {
    teacher: { sub: parseSub(row), email: text(row, "email"), name: text(row, "name") },
    expiresAt: integer(row, "expires_at"),
  };
}

export function deleteSession(sql: Sql, tokenHash: TokenHash): Promise<void> {
  return sql.run("DELETE FROM sessions WHERE token_hash = ?", [tokenHash]);
}

export async function getRefreshTokenEnc(sql: Sql, sub: Sub): Promise<string | null> {
  const row = await sql.first("SELECT refresh_token_enc FROM teachers WHERE sub = ?", [sub]);
  return row === null ? null : text(row, "refresh_token_enc");
}

export async function getDoc(sql: Sql, sub: Sub): Promise<string | null> {
  const row = await sql.first("SELECT doc FROM teachers WHERE sub = ?", [sub]);
  return row === null || row["doc"] === null ? null : text(row, "doc");
}

export function putDoc(sql: Sql, sub: Sub, doc: string, now: number): Promise<void> {
  return sql.run("UPDATE teachers SET doc = ?, updated_at = ? WHERE sub = ?", [doc, now, sub]);
}
