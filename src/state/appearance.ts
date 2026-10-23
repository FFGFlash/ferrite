/**
 * Visual settings from `settings.toml` that the frontend applies: theme and
 * editor font. The editor's line height derives from the font size.
 */
import type { FrontendSettings, Theme } from "../ipc/workspace";
import { applyTheme, followSystemTheme } from "../themes/theme";
import { createStore } from "./store";

export interface Appearance {
  theme: Theme;
  fontSize: number;
  fontFamily: string;
  tabSize: number;
  /** Editor line height in pixels. */
  lineHeight: number;
}

const lineHeightFor = (fontSize: number) => Math.round(fontSize * 1.45);

export const appearance = createStore<Appearance>({
  theme: "system",
  fontSize: 14,
  fontFamily: "",
  tabSize: 4,
  lineHeight: lineHeightFor(14),
});

let stopFollowingSystem: (() => void) | null = null;

export function applySettings(settings: FrontendSettings): void {
  appearance.set({ ...settings, lineHeight: lineHeightFor(settings.fontSize) });

  stopFollowingSystem?.();
  stopFollowingSystem = null;
  if (settings.theme === "system") {
    stopFollowingSystem = followSystemTheme();
  } else {
    applyTheme(settings.theme);
  }

  const root = document.documentElement.style;
  root.setProperty("--font-size-editor", `${settings.fontSize}px`);
  root.setProperty("--editor-line-height", `${lineHeightFor(settings.fontSize)}px`);
  root.setProperty("--editor-tab-size", String(settings.tabSize));
  if (settings.fontFamily.trim()) {
    root.setProperty("--font-mono", `${JSON.stringify(settings.fontFamily.trim())}, monospace`);
  } else {
    root.removeProperty("--font-mono");
  }
}
