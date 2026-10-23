/**
 * Theme selection. Themes are sets of semantic tokens defined in
 * `styles/tokens.css` and selected with the `data-theme` attribute on the
 * root element; components never reference concrete colors.
 */
export type ThemeKind = "dark" | "light";

export function applyTheme(kind: ThemeKind): void {
  document.documentElement.dataset.theme = kind;
}

/**
 * Follows the operating system's color scheme. Used until an explicit theme
 * setting exists. Returns a function that stops following.
 */
export function followSystemTheme(): () => void {
  const media = window.matchMedia("(prefers-color-scheme: light)");
  const update = () => applyTheme(media.matches ? "light" : "dark");
  update();
  media.addEventListener("change", update);
  return () => media.removeEventListener("change", update);
}
