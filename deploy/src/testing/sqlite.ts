import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import type { Sql } from "../store.ts";

const MIGRATIONS_DIR = join(import.meta.dirname, "../../migrations");

/**
 * An in-memory SQLite database with the real migrations applied, behind the same `Sql` as D1.
 * D1 is SQLite and enforces foreign keys, so the production SQL runs here unchanged.
 */
export function sqliteSql(): { sql: Sql; db: DatabaseSync } {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON");
  for (const file of readdirSync(MIGRATIONS_DIR).sort()) {
    db.exec(readFileSync(join(MIGRATIONS_DIR, file), "utf8"));
  }
  const sql: Sql = {
    first: async (query, params = []) => db.prepare(query).get(...params) ?? null,
    run: async (query, params = []) => {
      db.prepare(query).run(...params);
    },
  };
  return { sql, db };
}
