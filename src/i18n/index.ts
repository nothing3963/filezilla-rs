import { useCallback } from "react";
import { useAppStore } from "../store/app";
import { translate, type Translate } from "./translate";

export type { Translate };

export function useT(): Translate {
  const language = useAppStore((state) => state.settings.language);
  return useCallback(
    (key: string, vars?: Record<string, string | number>) => translate(language, key, vars),
    [language],
  );
}
