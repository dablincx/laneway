import { FluentBundle, FluentResource, type FluentVariable } from "@fluent/bundle";
import { negotiateLanguages } from "@fluent/langneg";
import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";
import { parse } from "smol-toml";

import languagesToml from "../../../locales/languages.toml?raw";

// Every locales/<code>.ftl file, bundled at build time.
const files = import.meta.glob<string>("../../../locales/*.ftl", {
  query: "?raw",
  import: "default",
  eager: true,
});

type LanguagesFile = { default: string; languages: Record<string, string> };
const config = parse(languagesToml) as unknown as LanguagesFile;

export const defaultLocale = config.default;
/** Code -> native name, for the language picker. */
export const languages: Record<string, string> = config.languages;

const bundles = new Map<string, FluentBundle>();
for (const [path, source] of Object.entries(files)) {
  const code = path
    .split("/")
    .pop()
    ?.replace(/\.ftl$/, "");
  if (!code || !(code in languages)) continue;
  const bundle = new FluentBundle(code, { useIsolating: false });
  const errors = bundle.addResource(new FluentResource(source));
  if (errors.length > 0) console.error(`locales/${code}.ftl`, errors);
  bundles.set(code, bundle);
}

export type Translate = (id: string, args?: Record<string, FluentVariable>) => string;

/** Looks the id up in the locale first, then English. Unknown ids come back as-is. */
export function translator(locale: string): Translate {
  const chain = [bundles.get(locale), bundles.get(defaultLocale)].filter((b) => b !== undefined);
  return (id, args) => {
    for (const bundle of chain) {
      const message = bundle.getMessage(id);
      if (message?.value) return bundle.formatPattern(message.value, args);
    }
    if (import.meta.env.DEV) console.warn(`missing translation: ${id}`);
    return id;
  };
}

/** Best match for the browser's languages. */
export function detectLocale(): string {
  const [best] = negotiateLanguages([...navigator.languages], Object.keys(languages), {
    defaultLocale,
    strategy: "lookup",
  });
  return best ?? defaultLocale;
}

type I18n = { locale: string; setLocale: (locale: string) => void; t: Translate };
const I18nContext = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState(detectLocale);
  const setLocale = useCallback((next: string) => {
    setLocaleState(next);
    document.documentElement.lang = next;
  }, []);
  const value = useMemo(() => ({ locale, setLocale, t: translator(locale) }), [locale, setLocale]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  const context = useContext(I18nContext);
  if (!context) throw new Error("useI18n needs <I18nProvider>");
  return context;
}
