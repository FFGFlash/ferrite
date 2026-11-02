/**
 * The window's zoom (View: Zoom In, Zoom Out, Reset Zoom), in the steps
 * browsers use. It's saved with the layout and applied whenever that
 * changes, including when the saved layout loads.
 */
import { setWindowZoom } from '../ipc'
import { layoutStore, updateLayout } from './layout'
import { reportError } from './notifications'

export const ZOOM_STEPS = [0.5, 0.67, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3]

/** The next step from `zoom` in `direction`; a saved zoom between steps goes to the nearest. */
export function nextZoom(zoom: number, direction: 1 | -1): number {
  const next =
    direction > 0
      ? ZOOM_STEPS.find(step => step > zoom + 0.001)
      : ZOOM_STEPS.filter(step => step < zoom - 0.001).at(-1)
  return next ?? zoom
}

export const zoomIn = () => updateLayout({ zoom: nextZoom(layoutStore.get().zoom, 1) })
export const zoomOut = () => updateLayout({ zoom: nextZoom(layoutStore.get().zoom, -1) })
export const resetZoom = () => updateLayout({ zoom: 1 })

let applied = 1

layoutStore.subscribe(() => {
  const { zoom } = layoutStore.get()
  if (zoom !== applied) {
    applied = zoom
    setWindowZoom(zoom).catch((error: unknown) => reportError('Zooming', error))
  }
})
