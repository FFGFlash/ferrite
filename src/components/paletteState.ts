import { createStore } from "../state/store";

export interface PaletteState {
  open: boolean;
  /** Initial input; its prefix selects the mode (`>` commands, `:` line). */
  initial: string;
  /** Changes on every open, so reopening resets the input. */
  session: number;
}

export const paletteStore = createStore<PaletteState>({ open: false, initial: "", session: 0 });

export function openPalette(initial: string): void {
  paletteStore.update((state) => ({ open: true, initial, session: state.session + 1 }));
}

export function closePalette(): void {
  paletteStore.update((state) => ({ ...state, open: false }));
}
