import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Locale = "en" | "zh";

const LOCALE_KEY = "mini-agent-locale";

type LocaleState = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
};

export const useLocaleStore = create<LocaleState>()(
  persist(
    (set) => ({
      locale: "en",
      setLocale: (locale) => set({ locale }),
    }),
    { name: LOCALE_KEY }
  )
);
