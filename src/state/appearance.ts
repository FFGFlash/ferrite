/**
 * Settings from `settings.toml` that the frontend applies: theme (built-in
 * or contributed by an extension) and color overrides, editor font, and
 * keybinding overrides. The editor's line height derives from the font
 * size.
 */
import { setKeybindingOverrides } from "../commands/registry";
import type { FrontendSettings, Theme } from "../ipc/workspace";
import { applyTheme, followSystemTheme, type ThemeKind } from "../themes/theme";
import { extensionsStore, usableExtensions } from "./extensions";
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
let current: FrontendSettings | null = null;

/** A theme's base and colors: built in, or from an enabled extension. */
function resolveTheme(id: Theme): { kind: ThemeKind | "system"; colors: Record<string, string> } {
  if (id === "system" || id === "dark" || id === "light") {
    return { kind: id, colors: {} };
  }
  for (const extension of usableExtensions(extensionsStore.get())) {
    const theme = extension.contributions.themes.find((candidate) => candidate.id === id);
    if (theme) {
      return { kind: theme.kind, colors: theme.colors };
    }
  }
  // Not loaded (yet): extensions load after settings, and this runs again.
  return { kind: "system", colors: {} };
}

function applyColors(settings: FrontendSettings): void {
  const theme = resolveTheme(settings.theme);
  const colors = { ...theme.colors, ...settings.colors };
  stopFollowingSystem?.();
  stopFollowingSystem = null;
  if (theme.kind === "system") {
    stopFollowingSystem = followSystemTheme(colors);
  } else {
    applyTheme(theme.kind, colors);
  }
}

export function applySettings(settings: FrontendSettings): void {
  current = settings;
  appearance.set({
    theme: settings.theme,
    fontSize: settings.fontSize,
    fontFamily: settings.fontFamily,
    tabSize: settings.tabSize,
    lineHeight: lineHeightFor(settings.fontSize),
  });
  applyColors(settings);
  setKeybindingOverrides(settings.keybindings);

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

// A contributed theme may load (or be disabled) after settings are applied.
extensionsStore.subscribe(() => {
  if (current) {
    applyColors(current);
  }
});
