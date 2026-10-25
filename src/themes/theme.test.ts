import { readFileSync } from 'node:fs'
import { afterEach, describe, expect, it } from 'vitest'
import { applyTheme, THEME_TOKENS, themeKind } from './theme'

describe('themes', () => {
  afterEach(() => applyTheme('dark'))

  // The light theme inherits what it doesn't redefine from the dark block,
  // which matches `:root`, so that block defines every token.
  it('lists exactly the color tokens the built-in themes define', () => {
    const tokensCss = readFileSync('src/styles/tokens.css', 'utf8')
    const block = (selector: string) => {
      const start = tokensCss.indexOf(selector)
      return tokensCss.slice(start, tokensCss.indexOf('}', start))
    }
    const defined = (css: string) =>
      new Set([...css.matchAll(/--color-([a-z0-9-]+):/g)].map(match => match[1]))
    const dark = defined(block('[data-theme="dark"]'))
    const light = defined(block('[data-theme="light"]'))
    expect([...THEME_TOKENS].sort()).toEqual([...dark].sort())
    expect([...light].every(token => dark.has(token))).toBe(true)
  })

  it('applies a base theme with color overrides, and only valid ones', () => {
    applyTheme('light', {
      'editor-background': '#101020',
      'text': 'red',
      'not-a-token': '#ffffff',
    })
    const root = document.documentElement
    expect(root.dataset.theme).toBe('light')
    expect(themeKind.get()).toBe('light')
    expect(root.style.getPropertyValue('--color-editor-background')).toBe('#101020')
    expect(root.style.getPropertyValue('--color-text')).toBe('')
    expect(root.style.getPropertyValue('--color-not-a-token')).toBe('')

    applyTheme('dark')
    expect(root.style.getPropertyValue('--color-editor-background')).toBe('')
  })
})
