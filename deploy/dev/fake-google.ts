/**
 * Local stand-in for Google sign-in and Classroom, so the whole Worker flow runs with no Google
 * Cloud project. Start it with `pnpm run fake-google`. It listens on :8788 (override with PORT).
 *
 * Point the Worker at it with these lines in `deploy/.dev.vars`, next to the three secrets:
 *
 *   GOOGLE_AUTHORIZE_URL=http://localhost:8788/authorize
 *   GOOGLE_TOKEN_URL=http://localhost:8788/token
 *   CLASSROOM_API_URL=http://localhost:8788
 *
 * Signing in skips the consent screen and returns straight to the Worker as one fixed teacher.
 * `POST /__revoke` makes the refresh token fail with invalid_grant, which sends the Worker's
 * Classroom route down its `reauthorize` path. `POST /__restore` undoes it. `POST /__courses`
 * takes a JSON array of `{ id, name, students: [{ id, name }] }` and makes it the Classroom the
 * Worker sees from then on.
 */
import { createServer } from "node:http";
import { createFakeGoogle, parseFakeCourses } from "./fake-google-core.ts";

const port = Number(process.env["PORT"] ?? 8788);
const google = createFakeGoogle();

const server = createServer(async (incoming, outgoing) => {
  const url = new URL(incoming.url ?? "/", `http://localhost:${port}`);
  if (incoming.method === "POST" && url.pathname === "/__revoke") {
    google.revokeRefreshToken();
    outgoing.writeHead(204).end();
    return;
  }
  if (incoming.method === "POST" && url.pathname === "/__restore") {
    google.restoreRefreshToken();
    outgoing.writeHead(204).end();
    return;
  }

  const chunks: Buffer[] = [];
  for await (const chunk of incoming) {
    chunks.push(chunk as Buffer);
  }
  if (incoming.method === "POST" && url.pathname === "/__courses") {
    let parsed: unknown = null;
    try {
      parsed = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    } catch {
      // Not JSON, so it fails the shape check below.
    }
    const courses = parseFakeCourses(parsed);
    if (courses === null) {
      outgoing
        .writeHead(400)
        .end("expected a JSON array of { id, name, students: [{ id, name }] }\n");
      return;
    }
    google.setCourses(courses);
    outgoing.writeHead(204).end();
    return;
  }
  const hasBody = incoming.method !== "GET" && incoming.method !== "HEAD";
  const response = await google.fetch(
    new Request(url, {
      method: incoming.method ?? "GET",
      headers: incoming.headers as Record<string, string>,
      body: hasBody ? Buffer.concat(chunks) : undefined,
    }),
  );
  outgoing.writeHead(response.status, Object.fromEntries(response.headers));
  outgoing.end(Buffer.from(await response.arrayBuffer()));
  console.log(`${incoming.method} ${url.pathname} ${response.status}`);
});

server.listen(port, () => {
  console.log(`fake google listening on http://localhost:${port}`);
});
