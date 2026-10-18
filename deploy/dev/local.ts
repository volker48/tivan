/**
 * Fixed D1 id for `cf dev --mode local`. `cf d1 migrations apply` only accepts a database id, and
 * the local database file is named after it, so the dev server and the migration command have to
 * agree on one. The `db:migrate:local` script in `package.json` repeats this value.
 */
export const LOCAL_D1_ID = "6d1c2a0e-5b3f-4c8a-9e7d-1a2b3c4d5e6f";
