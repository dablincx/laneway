import { readFileSync } from "node:fs";
import { FluentBundle, FluentResource, type FluentVariable } from "@fluent/bundle";

// Expected UI text comes from the same locales/*.ftl files the UI uses, so the tests repeat no
// strings and keep passing when the wording changes.
const bundles = new Map<string, FluentBundle>();

function bundle(locale: string): FluentBundle {
  let found = bundles.get(locale);
  if (!found) {
    const source = readFileSync(new URL(`../../locales/${locale}.ftl`, import.meta.url), "utf8");
    found = new FluentBundle(locale, { useIsolating: false });
    found.addResource(new FluentResource(source));
    bundles.set(locale, found);
  }
  return found;
}

/** The message in that locale. Fails if the id is missing there, so a test can't pass on the English fallback. */
export function text(locale: string, id: string, args?: Record<string, FluentVariable>): string {
  const b = bundle(locale);
  const message = b.getMessage(id);
  if (!message?.value) throw new Error(`no message ${id} in locales/${locale}.ftl`);
  return b.formatPattern(message.value, args);
}
