/**
 * Theme selection. Themes are sets of semantic tokens defined in
 * `styles/tokens.css` and selected with the `data-theme` attribute on the
 * root element; components never reference concrete colors. Extension
 * themes and the user's `workbench.colors` override individual tokens on
 * top of the dark or light base.
 */
import { createStore } from '../state/store'
import tokenList from './tokens.json'

export type ThemeKind = 'dark' | 'light'

/** The tokens a theme may set (`--color-<token>`), shared with the backend. */
export const THEME_TOKENS: ReadonlySet<string> = new Set(tokenList)

const HEX_COLOR = /^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i

/** The base theme in effect, for code that picks colors per theme. */
export const themeKind = createStore<ThemeKind>('dark')
/**
 * Counts theme changes, including overrides that keep the base, for code
 * that reads resolved token values (such as the terminal's colors).
 */
export const themeRevision = createStore(0)

let overridden: string[] = []

/**
 * Applies a base theme and token overrides. Overrides were validated by the
 * backend; unknown tokens and non-hex values are skipped here as well, so
 * nothing but a color can reach the stylesheet.
 */
export function applyTheme(kind: ThemeKind, colors: Readonly<Record<string, string>> = {}): void {
  const root = document.documentElement
  root.dataset.theme = kind
  for (const token of overridden) {
    root.style.removeProperty(`--color-${token}`)
  }
  overridden = []
  for (const [token, value] of Object.entries(colors)) {
    if (THEME_TOKENS.has(token) && HEX_COLOR.test(value)) {
      root.style.setProperty(`--color-${token}`, value)
      overridden.push(token)
    }
  }
  themeKind.set(kind)
  themeRevision.update(revision => revision + 1)
}

/**
 * Follows the operating system's color scheme, with `colors` on top.
 * Returns a function that stops following.
 */
export function followSystemTheme(colors: Readonly<Record<string, string>> = {}): () => void {
  const media = window.matchMedia('(prefers-color-scheme: light)')
  const update = () => applyTheme(media.matches ? 'light' : 'dark', colors)
  update()
  media.addEventListener('change', update)
  return () => media.removeEventListener('change', update)
}
