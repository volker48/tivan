import { describe, expect, it } from "vitest";
import { parseConfig } from "./config.ts";
import { TEST_ENV } from "./testing/app.ts";

describe("parseConfig", () => {
  it("parses a complete environment", () => {
    const result = parseConfig(TEST_ENV);
    expect(result.kind).toBe("ok");
    if (result.kind === "ok") {
      expect(result.config.clientId).toBe(TEST_ENV.GOOGLE_CLIENT_ID);
      expect(result.config.tokenKey).toHaveLength(32);
      expect(result.config.tokenUrl.href).toBe("https://google.test/token");
      expect(result.config.classroomUrl.origin).toBe("https://google.test");
    }
  });

  it("reports every missing secret by name", () => {
    const result = parseConfig({ ...TEST_ENV, GOOGLE_CLIENT_ID: undefined, TOKEN_ENC_KEY: "" });
    expect(result).toEqual({
      kind: "invalid",
      problems: [
        "GOOGLE_CLIENT_ID is missing",
        "TOKEN_ENC_KEY is missing or not base64 of 32 bytes",
      ],
    });
  });

  it.each([
    ["a key of the wrong length", { TOKEN_ENC_KEY: btoa("too short") }],
    ["a key that is not base64", { TOKEN_ENC_KEY: "***" }],
    ["a non-string secret", { GOOGLE_CLIENT_SECRET: 42 }],
    ["a URL that does not parse", { GOOGLE_TOKEN_URL: "not a url" }],
    ["a URL with another scheme", { CLASSROOM_API_URL: "ftp://classroom.example" }],
  ])("rejects %s", (_label, override) => {
    expect(parseConfig({ ...TEST_ENV, ...override }).kind).toBe("invalid");
  });

  it("never echoes a value in its problems", () => {
    const result = parseConfig({
      ...TEST_ENV,
      TOKEN_ENC_KEY: "super-secret-but-wrong",
      GOOGLE_TOKEN_URL: "https://user:password@",
    });
    expect(result.kind).toBe("invalid");
    expect(JSON.stringify(result)).not.toContain("super-secret-but-wrong");
    expect(JSON.stringify(result)).not.toContain("password");
  });
});
