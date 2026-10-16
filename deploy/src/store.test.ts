import { describe, expect, it } from "vitest";
import { sqliteSql } from "./testing/sqlite.ts";
import {
  deleteSession,
  findSession,
  getDoc,
  getRefreshTokenEnc,
  insertSession,
  putDoc,
  upsertTeacher,
} from "./store.ts";
import { type Teacher, type TokenHash, toSub } from "./types.ts";

function teacher(sub: string, email = `${sub}@example.test`): Teacher {
  const parsed = toSub(sub);
  if (parsed === null) {
    throw new Error("sub must not be empty");
  }
  return { sub: parsed, email, name: `Teacher ${sub}` };
}

const hash = (value: string): TokenHash => value as TokenHash;

describe("teachers", () => {
  it("upsert refreshes the profile and token but never the doc", async () => {
    const { sql } = sqliteSql();
    const ada = teacher("ada");
    await upsertTeacher(sql, ada, "sealed-1", 1);
    await putDoc(sql, ada.sub, '{"version":5,"doc":{}}', 2);

    await upsertTeacher(sql, { ...ada, email: "new@example.test" }, "sealed-2", 3);

    expect(await getDoc(sql, ada.sub)).toBe('{"version":5,"doc":{}}');
    expect(await getRefreshTokenEnc(sql, ada.sub)).toBe("sealed-2");
    await insertSession(sql, hash("h"), ada.sub, 10, 3);
    expect((await findSession(sql, hash("h")))?.teacher.email).toBe("new@example.test");
  });

  it("keeps docs separate per teacher", async () => {
    const { sql } = sqliteSql();
    const [ada, grace] = [teacher("ada"), teacher("grace")];
    await upsertTeacher(sql, ada, "a", 1);
    await upsertTeacher(sql, grace, "g", 1);

    await putDoc(sql, ada.sub, "ada-doc", 2);

    expect(await getDoc(sql, ada.sub)).toBe("ada-doc");
    expect(await getDoc(sql, grace.sub)).toBeNull();
  });

  it("answers null for a teacher that does not exist", async () => {
    const { sql } = sqliteSql();
    const nobody = teacher("nobody");
    expect(await getDoc(sql, nobody.sub)).toBeNull();
    expect(await getRefreshTokenEnc(sql, nobody.sub)).toBeNull();
  });
});

describe("sessions", () => {
  it("are found by hash with their teacher and expiry", async () => {
    const { sql } = sqliteSql();
    const ada = teacher("ada");
    await upsertTeacher(sql, ada, "a", 1);
    await insertSession(sql, hash("h1"), ada.sub, 100, 1);

    expect(await findSession(sql, hash("h1"))).toEqual({ teacher: ada, expiresAt: 100 });
    expect(await findSession(sql, hash("other"))).toBeNull();
  });

  it("are deleted individually", async () => {
    const { sql } = sqliteSql();
    const ada = teacher("ada");
    await upsertTeacher(sql, ada, "a", 1);
    await insertSession(sql, hash("h1"), ada.sub, 100, 1);
    await insertSession(sql, hash("h2"), ada.sub, 100, 1);

    await deleteSession(sql, hash("h1"));

    expect(await findSession(sql, hash("h1"))).toBeNull();
    expect(await findSession(sql, hash("h2"))).not.toBeNull();
  });

  it("are removed with their teacher by the foreign key", async () => {
    const { sql, db } = sqliteSql();
    const ada = teacher("ada");
    await upsertTeacher(sql, ada, "a", 1);
    await insertSession(sql, hash("h1"), ada.sub, 100, 1);

    db.exec("DELETE FROM teachers WHERE sub = 'ada'");

    expect(await findSession(sql, hash("h1"))).toBeNull();
  });

  it("cannot be created for an unknown teacher", async () => {
    const { sql } = sqliteSql();
    await expect(insertSession(sql, hash("h1"), teacher("ghost").sub, 100, 1)).rejects.toThrow();
  });

  it("creating one sweeps sessions that already expired", async () => {
    const { sql } = sqliteSql();
    const ada = teacher("ada");
    await upsertTeacher(sql, ada, "a", 1);
    await insertSession(sql, hash("old"), ada.sub, 50, 1);

    await insertSession(sql, hash("new"), ada.sub, 200, 50);

    expect(await findSession(sql, hash("old"))).toBeNull();
    expect(await findSession(sql, hash("new"))).not.toBeNull();
  });
});
