import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";
import { LOCAL_D1_ID } from "./local.ts";

it("migrates the same local database that `cf dev --mode local` serves", () => {
  const manifest = JSON.parse(
    readFileSync(join(import.meta.dirname, "../package.json"), "utf8"),
  ) as { scripts: Record<string, string> };
  expect(manifest.scripts["db:migrate:local"]).toContain(` ${LOCAL_D1_ID} `);
  expect(LOCAL_D1_ID).toMatch(
    /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
  );
});
