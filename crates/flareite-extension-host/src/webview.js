// The `flareite` object a webview page uses to talk to the editor. The host
// serves it to every page, before the page's own scripts, after a line
// that sets `FLAREITE_PAGE`: what the host knows about this page.
//
// The page runs in a sandboxed frame with an origin of its own, so its only
// way out is `postMessage` to the workbench, which checks that messages
// come from the frame it made and acts on them.
;(() => {
  // The webview injects the editor's IPC script into every frame. It can't
  // reach the editor from here: the policy blocks the `ipc:` protocol, and
  // WebView2 doesn't deliver frames' web messages. Its fallback channel goes
  // too, so nothing depends on the latter. (`__TAURI_INTERNALS__` itself
  // can't be deleted, and has no way out left.)
  if (window.chrome?.webview) {
    delete window.chrome.webview
  }

  const page = FLAREITE_PAGE
  const parent = window.parent
  const send = (type, data) => parent.postMessage({ flareite: 'webview', type, data }, '*')

  const messageListeners = new Set()
  const fileListeners = new Set()
  const themeListeners = new Set()
  const storageListeners = new Set()
  const actionListeners = new Set()
  // Messages that arrived before the page listened for them.
  let waiting = []
  let fileVersion = 0
  let state = page.state
  let themeKind = page.theme || 'dark'
  document.documentElement.dataset.flareiteTheme = themeKind

  const json = (value, what) => {
    const text = JSON.stringify(value)
    if (text === undefined) {
      throw new TypeError(`${what} needs a value JSON can represent`)
    }
    return text
  }
  const notify = (listeners, value) => {
    for (const listener of listeners) {
      listener(value)
    }
  }
  const listen = (listeners, listener) => {
    listeners.add(listener)
    return () => listeners.delete(listener)
  }

  // Storage requests waiting for the workbench's answer, by id.
  const requests = new Map()
  let nextRequest = 0
  const request = (scope, op, key, value) =>
    new Promise((resolve, reject) => {
      const id = nextRequest++
      requests.set(id, { resolve, reject })
      send('storage', { id, scope, op, key, value })
    })
  const storage = scope =>
    Object.freeze({
      /** A stored value, or `undefined` if there is none. */
      get: key =>
        request(scope, 'get', key).then(text => (text == null ? undefined : JSON.parse(text))),
      /** Stores a value JSON can represent; `undefined` removes it. */
      set: (key, value) =>
        request(scope, 'set', key, value === undefined ? null : json(value, 'storage.set')),
      /** The keys that have values, sorted. */
      keys: () => request(scope, 'keys'),
    })

  // The workbench's shortcuts, which a page passes on unless it handles the
  // key itself (`event.preventDefault()` in a listener on an element or the
  // document); they're matched as the workbench matches them.
  let keystrokes = []
  let chordPending = false
  const MODIFIERS = new Set(['Control', 'Shift', 'Alt', 'AltGraph', 'Meta', 'CapsLock', 'OS'])
  const PUNCTUATION = {
    Backquote: '`',
    Minus: '-',
    Equal: '=',
    BracketLeft: '[',
    BracketRight: ']',
    Backslash: '\\',
    Semicolon: ';',
    Quote: "'",
    Comma: ',',
    Period: '.',
    Slash: '/',
  }
  const unshifted = code => PUNCTUATION[code] ?? /^Digit(\d)$/.exec(code)?.[1]
  const letter = code => /^Key([A-Z])$/.exec(code)?.[1]?.toLowerCase()
  const matches = (stroke, event) => {
    const key = event.key === ' ' ? 'space' : event.key.toLowerCase()
    return (
      (stroke.key === key ||
        (event.shiftKey && stroke.key === unshifted(event.code)) ||
        (event.altKey &&
          (stroke.key === letter(event.code) || stroke.key === unshifted(event.code)))) &&
      stroke.ctrl === event.ctrlKey &&
      stroke.meta === event.metaKey &&
      stroke.alt === event.altKey &&
      stroke.shift === event.shiftKey
    )
  }
  window.addEventListener('keydown', event => {
    if (event.defaultPrevented || event.isComposing || MODIFIERS.has(event.key)) {
      return
    }
    // The second key of a chord goes to the workbench whatever it is.
    if (!chordPending && !keystrokes.some(stroke => matches(stroke, event))) {
      return
    }
    event.preventDefault()
    const { key, code, ctrlKey, shiftKey, altKey, metaKey, repeat } = event
    send('keydown', { key, code, ctrlKey, shiftKey, altKey, metaKey, repeat })
  })

  window.addEventListener('message', event => {
    const message = event.data
    if (event.source !== parent || message?.flareite !== 'webview') {
      return
    }
    switch (message.type) {
      case 'message': {
        const data = JSON.parse(message.data)
        if (messageListeners.size === 0) {
          waiting.push(data)
        } else {
          notify(messageListeners, data)
        }
        break
      }
      case 'fileChanged':
        fileVersion += 1
        notify(fileListeners, flareite.fileUrl)
        break
      case 'storageResult': {
        const { id, value, error } = message.data
        const pending = requests.get(id)
        requests.delete(id)
        if (error === undefined) {
          pending?.resolve(value)
        } else {
          pending?.reject(new Error(error))
        }
        break
      }
      case 'storageChanged': {
        const { scope, key, value } = message.data
        notify(storageListeners, {
          scope,
          key,
          value: value == null ? undefined : JSON.parse(value),
        })
        break
      }
      case 'theme': {
        const { kind, variables } = message.data
        // As the host writes it (`WebviewTheme::css`), so an unchanged theme
        // is recognized and listeners aren't told.
        let css = `:root { --flareite-color-scheme: ${kind};`
        for (const name of Object.keys(variables).sort()) {
          css += ` --flareite-${name}: ${variables[name]};`
        }
        css += ' }'
        const style = document.getElementById('flareite-theme')
        if (style?.textContent === css) {
          break
        }
        if (style) {
          style.textContent = css
        }
        themeKind = kind
        document.documentElement.dataset.flareiteTheme = kind
        notify(themeListeners, kind)
        break
      }
      case 'action':
        notify(actionListeners, message.data)
        break
      case 'keybindings':
        keystrokes = message.data
        break
      case 'chord':
        chordPending = message.data
        break
    }
  })

  const flareite = {
    /** `viewer` or `preview`. */
    kind: page.kind,
    /**
     * In a viewer, the address of the file open in it (it changes when the
     * file does, so the browser loads it again); `null` in a preview.
     */
    get fileUrl() {
      return page.file === null ? null : `${page.file}?v=${fileVersion}`
    },
    /** In a viewer, the file's name. */
    fileName: page.fileName,
    /** `dark` or `light`: the workbench's theme (also `data-flareite-theme` on the root). */
    get theme() {
      return themeKind
    },
    /**
     * Sends a message to the extension, which receives it as JSON text in
     * the command its manifest names as `on_message`.
     */
    postMessage(data) {
      send('message', json(data, 'flareite.postMessage'))
    },
    /**
     * Calls `listener` with each message the extension sends, parsed.
     * Messages sent before the first listener wait for it. Returns a
     * function that stops listening.
     */
    onMessage(listener) {
      messageListeners.add(listener)
      if (waiting.length > 0) {
        const delivered = waiting
        waiting = []
        queueMicrotask(() => delivered.forEach(data => listener(data)))
      }
      return () => messageListeners.delete(listener)
    },
    /**
     * In a viewer, calls `listener` with the file's new address when the
     * file changes on disk. Returns a function that stops listening.
     */
    onFileChanged: listener => listen(fileListeners, listener),
    /**
     * Calls `listener` with the theme's kind when the workbench's theme
     * changes; the `--flareite-*` variables have changed by then. Returns a
     * function that stops listening.
     */
    onThemeChanged: listener => listen(themeListeners, listener),
    /**
     * What this tab's page saved with `setState`, or `undefined`. The page
     * gets it back when it loads again: after its tab was hidden, and in a
     * viewer, in the next session.
     */
    getState() {
      return state ?? undefined
    },
    /** Saves this tab's state: a value JSON can represent, at most 64 KiB. */
    setState(value) {
      const text = json(value, 'flareite.setState')
      state = JSON.parse(text)
      send('setState', text)
    },
    /**
     * Values the extension keeps across sessions, shared with its code and
     * its other pages. Each call returns a promise.
     */
    storage: storage('global'),
    /** As `storage`, but kept for the folder open in the editor. */
    workspaceStorage: storage('workspace'),
    /**
     * Calls `listener` with `{ scope, key, value }` when a stored value
     * changes elsewhere: in the extension's code or another of its pages.
     * `value` is `undefined` when it was removed; `scope` and `key` are
     * `null` when the user cleared everything. Returns a function that stops
     * listening.
     */
    onStorageChanged: listener => listen(storageListeners, listener),
    /**
     * Calls `listener` with an action's id when one of the tab's actions
     * without a command is clicked. Returns a function that stops listening.
     */
    onAction: listener => listen(actionListeners, listener),
    /**
     * Sets how one of the tab's actions looks: any of `pressed`, `disabled`,
     * and `hidden` (booleans), and `title`.
     */
    setActionState(action, actionState) {
      send('actionState', { action, state: actionState })
    },
  }
  Object.defineProperty(window, 'flareite', { value: Object.freeze(flareite) })
  send('ready')
})()
