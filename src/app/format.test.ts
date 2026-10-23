import { describe, expect, it } from "vitest";
import { formatPlatform } from "./format";

describe("formatPlatform", () => {
  it("uses display names for known operating systems", () => {
    expect(formatPlatform({ os: "macos", arch: "aarch64" })).toBe("macOS aarch64");
    expect(formatPlatform({ os: "linux", arch: "x86_64" })).toBe("Linux x86_64");
  });

  it("falls back to the raw identifier for unknown operating systems", () => {
    expect(formatPlatform({ os: "freebsd", arch: "x86_64" })).toBe("freebsd x86_64");
  });
});
