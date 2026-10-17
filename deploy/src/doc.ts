import { isRecord } from "./json.ts";

export const MAX_DOC_BYTES = 1024 * 1024;

export type DocBody =
  | { readonly kind: "ok"; readonly text: string }
  | { readonly kind: "too_large" }
  | { readonly kind: "invalid" };

/** Reads at most `MAX_DOC_BYTES` of the body. The header can be absent or wrong, so the bytes are counted. */
async function readCapped(request: Request): Promise<Uint8Array | null> {
  const declared = Number(request.headers.get("content-length"));
  if (declared > MAX_DOC_BYTES) {
    return null;
  }
  const reader = request.body?.getReader();
  if (reader === undefined) {
    return new Uint8Array();
  }
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    total += value.byteLength;
    if (total > MAX_DOC_BYTES) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const body = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return body;
}

/**
 * The server only needs a persist envelope `{"version": number, "doc": object}` and stores the
 * exact text it was sent, so a stored doc reads back byte for byte.
 */
function isEnvelope(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value["version"] === "number" &&
    Number.isFinite(value["version"]) &&
    isRecord(value["doc"])
  );
}

export async function readDocBody(request: Request): Promise<DocBody> {
  const bytes = await readCapped(request);
  if (bytes === null) {
    return { kind: "too_large" };
  }
  try {
    const text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
    return isEnvelope(JSON.parse(text)) ? { kind: "ok", text } : { kind: "invalid" };
  } catch {
    return { kind: "invalid" };
  }
}
