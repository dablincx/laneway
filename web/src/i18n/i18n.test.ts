import { describe, expect, it } from "vitest";

import { defaultLocale, languages, translator } from "./index";

describe("translator", () => {
  it("uses the locale when the string is translated", () => {
    expect(translator("de")("common-version", { version: "1.0.0" })).toBe("Version 1.0.0");
  });

  it("falls back to english for untranslated strings", () => {
    // common-app-name only exists in en.ftl
    expect(translator("de")("common-app-name")).toBe("laneway");
  });

  it("returns the id for unknown strings", () => {
    expect(translator("fr")("does-not-exist")).toBe("does-not-exist");
  });

  it("knows the shipped languages", () => {
    expect(defaultLocale).toBe("en");
    expect(Object.keys(languages)).toEqual(["en", "de", "fr"]);
  });
});
