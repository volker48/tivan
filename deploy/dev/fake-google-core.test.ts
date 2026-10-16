import { describe, expect, it } from "vitest";
import { FAKE_COURSES, parseFakeCourses } from "./fake-google-core.ts";

describe("parseFakeCourses", () => {
  it("accepts the shape of the bundled courses, and an empty list", () => {
    expect(parseFakeCourses(JSON.parse(JSON.stringify(FAKE_COURSES)))).toEqual(FAKE_COURSES);
    expect(parseFakeCourses([])).toEqual([]);
  });

  it.each([
    ["an object", {}],
    ["null", null],
    ["a course without students", [{ id: "c", name: "n" }]],
    ["a course with an empty id", [{ id: "", name: "n", students: [] }]],
    ["a course whose name is not text", [{ id: "c", name: 1, students: [] }]],
    ["a student without an id", [{ id: "c", name: "n", students: [{ name: "x" }] }]],
    ["a student that is not an object", [{ id: "c", name: "n", students: ["x"] }]],
  ])("rejects %s", (_label, value) => {
    expect(parseFakeCourses(value)).toBeNull();
  });
});
