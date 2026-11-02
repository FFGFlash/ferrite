import { createStore } from '../state/store'

/** A choice in a quick pick. */
export interface QuickPickItem {
  label: string
  /** Shown dimmed after the label. */
  detail?: string
  run: () => void
}

/** A list to choose from in the palette, such as encodings. */
export interface QuickPick {
  /** Says what's being chosen; the input's placeholder and accessible name. */
  placeholder: string
  items: readonly QuickPickItem[]
}

export interface PaletteState {
  open: boolean
  /** Initial input; its prefix selects the mode (`>` commands, `:` line). */
  initial: string
  /** Changes on every open, so reopening resets the input. */
  session: number
  /** Set when the palette shows a quick pick instead of its modes. */
  pick: QuickPick | null
}

export const paletteStore = createStore<PaletteState>({
  open: false,
  initial: '',
  session: 0,
  pick: null,
})

export function openPalette(initial: string): void {
  paletteStore.update(state => ({ open: true, initial, session: state.session + 1, pick: null }))
}

/** Shows `pick` in the palette; typing filters its items. */
export function openQuickPick(pick: QuickPick): void {
  paletteStore.update(state => ({ open: true, initial: '', session: state.session + 1, pick }))
}

export function closePalette(): void {
  paletteStore.update(state => ({ ...state, open: false }))
}
