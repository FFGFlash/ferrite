import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach, vi } from 'vitest'

// xterm.js measures with a canvas, which jsdom lacks; tests use a fake.
vi.mock('@xterm/xterm', async () => ({ Terminal: (await import('./fakeXterm')).FakeTerminal }))
vi.mock('@xterm/addon-fit', async () => ({ FitAddon: (await import('./fakeXterm')).FakeFitAddon }))

// jsdom does not implement layout; scrolling is a no-op in tests.
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

// Nor media queries: the system color scheme is always dark in tests.
window.matchMedia ??= (query: string) =>
  ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  }) as MediaQueryList

// Nor resize observation: without layout, nothing ever resizes.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
}

afterEach(() => {
  cleanup()
})
