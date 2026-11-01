/**
 * Requests to show a view (the explorer, an extension's view) or a page
 * (the settings editor), answered by the dock. State modules use this
 * instead of importing the dock, which imports them.
 */
let handler: ((id: string, focus: boolean) => void) | null = null
let pageHandler: ((id: string) => void) | null = null

export function setViewHandler(show: (id: string, focus: boolean) => void): void {
  handler = show
}

export function setPageHandler(show: (id: string) => void): void {
  pageHandler = show
}

/** Shows the view `id` wherever it's docked (opening it if it isn't). */
export function showView(id: string, focus = false): void {
  handler?.(id, focus)
}

/** Shows and focuses the page `id`, opening it where documents open if it isn't open. */
export function showPage(id: string): void {
  pageHandler?.(id)
}
