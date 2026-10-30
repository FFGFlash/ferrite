/**
 * Requests to show a view (the explorer, an extension's view), answered by
 * the dock. State modules use this instead of importing the dock, which
 * imports them.
 */
let handler: ((id: string, focus: boolean) => void) | null = null

export function setViewHandler(show: (id: string, focus: boolean) => void): void {
  handler = show
}

/** Shows the view `id` wherever it's docked (opening it if it isn't). */
export function showView(id: string, focus = false): void {
  handler?.(id, focus)
}
