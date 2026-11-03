// The Images viewer page. It shows the file open in the viewer
// (`ferrite.fileUrl`) centered on a background, fit to the tab or at a zoom
// level, and asks the extension for the picture's details.
//
// - The background is the same in every image tab, and kept across
//   sessions: it's stored with `ferrite.storage`, and other tabs follow a
//   change (`ferrite.onStorageChanged`).
// - The zoom level is each tab's own (`ferrite.setState`).
// - Ctrl+wheel zooms about the pointer; +, -, 0, and F (with or without
//   Ctrl) zoom in, out, to actual size, and to fit.

/** Zoom levels the buttons and keys step through. */
const ZOOMS = [0.1, 0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4, 6, 8, 12, 16, 24, 32]
const MIN_ZOOM = ZOOMS[0]
const MAX_ZOOM = ZOOMS[ZOOMS.length - 1]
/** Space around the picture, matching the stage's padding. */
const PADDING = 16

const BACKGROUNDS = [
  ['checkerboard', 'Checkerboard'],
  ['light-checkerboard', 'Light Checkerboard'],
  ['dark-checkerboard', 'Dark Checkerboard'],
  ['none', 'No Background'],
  ['white', 'White'],
  ['black', 'Black'],
]
const isBackground = value => BACKGROUNDS.some(([name]) => name === value)

const stage = document.getElementById('stage')
const picture = document.getElementById('picture')
const problem = document.getElementById('problem')
const zoomLabel = document.getElementById('zoom')
const details = document.getElementById('details')
const buttons = {
  zoomOut: document.getElementById('zoom-out'),
  zoomIn: document.getElementById('zoom-in'),
  fit: document.getElementById('fit'),
  actualSize: document.getElementById('actual-size'),
}

/** SVG pictures are drawn at any size; bitmaps are fit only by shrinking them. */
const vector = /\.svg$/i.test(ferrite.fileName ?? '')
const saved = ferrite.getState() ?? {}
/** `'fit'`, or the scale: 1 is actual size. */
let zoom = saved.zoom === 'fit' || typeof saved.zoom === 'number' ? saved.zoom : 'fit'
let background = isBackground(saved.background) ? saved.background : 'checkerboard'
/** An SVG picture's size from the extension, which reads it from the file. */
let vectorSize = null

const save = () => ferrite.setState({ zoom, background })

/** The picture's size at 1:1, once known. */
function naturalSize() {
  if (vector && vectorSize) {
    return vectorSize
  }
  // A bitmap's own size, turned as its EXIF orientation says.
  return picture.naturalWidth > 0
    ? { width: picture.naturalWidth, height: picture.naturalHeight }
    : null
}

/** The scale the picture is drawn at now. */
function currentScale() {
  if (zoom !== 'fit') {
    return zoom
  }
  const size = naturalSize()
  if (!size) {
    return 1
  }
  const fits = Math.min(
    (stage.clientWidth - 2 * PADDING) / size.width,
    (stage.clientHeight - 2 * PADDING) / size.height,
  )
  return Math.max(vector ? fits : Math.min(1, fits), 0.01)
}

/** Sizes the picture and updates the toolbar. */
function layout() {
  const size = naturalSize()
  const scale = currentScale()
  if (size) {
    picture.style.width = `${size.width * scale}px`
    picture.style.height = `${size.height * scale}px`
  }
  picture.classList.toggle('pixelated', !vector && scale > 1)
  zoomLabel.textContent = zoom === 'fit' ? 'Fit' : `${Math.round(zoom * 100)}%`
  buttons.fit.setAttribute('aria-pressed', String(zoom === 'fit'))
  buttons.actualSize.setAttribute('aria-pressed', String(zoom === 1))
  buttons.zoomOut.disabled = scale <= MIN_ZOOM
  buttons.zoomIn.disabled = scale >= MAX_ZOOM
}

/**
 * Zooms to `next` (`'fit'` or a scale), keeping the point under `anchor`
 * (client coordinates; the stage's center by default) where it is.
 */
function setZoom(next, anchor) {
  const before = currentScale()
  const bounds = stage.getBoundingClientRect()
  const x = (anchor?.x ?? bounds.left + stage.clientWidth / 2) - bounds.left
  const y = (anchor?.y ?? bounds.top + stage.clientHeight / 2) - bounds.top
  // The anchor's point in the picture, at 1:1.
  const pointX = (stage.scrollLeft + x - picture.offsetLeft) / before
  const pointY = (stage.scrollTop + y - picture.offsetTop) / before
  zoom = next === 'fit' ? 'fit' : Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, next))
  layout()
  const after = currentScale()
  stage.scrollLeft = pointX * after + picture.offsetLeft - x
  stage.scrollTop = pointY * after + picture.offsetTop - y
  save()
}

const zoomIn = () => setZoom(ZOOMS.find(level => level > currentScale() * 1.001) ?? MAX_ZOOM)
const zoomOut = () =>
  setZoom([...ZOOMS].reverse().find(level => level < currentScale() / 1.001) ?? MIN_ZOOM)

function setBackground(next, store) {
  background = next
  document.body.dataset.background = next
  for (const swatch of document.querySelectorAll('[data-swatch]')) {
    swatch.setAttribute('aria-pressed', String(swatch.dataset.swatch === next))
  }
  save()
  if (store) {
    ferrite.storage.set('background', next)
  }
}

// The background swatches.
const swatches = document.getElementById('backgrounds')
for (const [name, label] of BACKGROUNDS) {
  const button = document.createElement('button')
  button.type = 'button'
  button.className = 'ferrite-button'
  button.dataset.swatch = name
  button.title = label
  button.setAttribute('aria-label', label)
  const swatch = document.createElement('span')
  swatch.className = 'swatch'
  swatch.dataset.background = name
  button.append(swatch)
  button.addEventListener('click', () => setBackground(name, true))
  swatches.append(button)
}
setBackground(background, false)
ferrite.storage.get('background').then(stored => {
  if (isBackground(stored) && stored !== background) {
    setBackground(stored, false)
  }
})
ferrite.onStorageChanged(({ scope, key, value }) => {
  // Changed in another tab, or cleared by the user.
  if (scope === null || (scope === 'global' && key === 'background')) {
    setBackground(isBackground(value) ? value : 'checkerboard', false)
  }
})

buttons.zoomIn.addEventListener('click', zoomIn)
buttons.zoomOut.addEventListener('click', zoomOut)
buttons.fit.addEventListener('click', () => setZoom('fit'))
buttons.actualSize.addEventListener('click', () => setZoom(1))

stage.addEventListener(
  'wheel',
  event => {
    if (!event.ctrlKey && !event.metaKey) {
      return
    }
    event.preventDefault()
    setZoom(currentScale() * Math.exp(-event.deltaY * 0.002), {
      x: event.clientX,
      y: event.clientY,
    })
  },
  { passive: false },
)

// On the document, so Ferrite's own shortcuts for these keys (zooming the
// window) don't run while the picture has focus.
document.addEventListener('keydown', event => {
  if (event.altKey || event.target instanceof HTMLInputElement) {
    return
  }
  const actions = {
    '+': zoomIn,
    '=': zoomIn,
    '-': zoomOut,
    '0': () => setZoom(1),
    'f': () => setZoom('fit'),
  }
  const action = actions[event.key.toLowerCase()]
  if (action) {
    event.preventDefault()
    action()
  }
})

new ResizeObserver(() => {
  if (zoom === 'fit') {
    layout()
  }
}).observe(stage)

picture.addEventListener('load', () => {
  picture.hidden = false
  problem.hidden = true
  layout()
})
picture.addEventListener('error', () => {
  picture.hidden = true
  problem.hidden = false
  problem.textContent = `${ferrite.fileName} can't be shown: it isn't a picture this viewer can draw.`
})

// The details line, and an SVG picture's size.
ferrite.onMessage(message => {
  if (typeof message.details === 'string') {
    details.textContent = message.details
    details.title = message.details
  }
  vectorSize =
    message.width > 0 && message.height > 0
      ? { width: message.width, height: message.height }
      : null
  layout()
})
const askForDetails = () => ferrite.postMessage({ type: 'details' })

ferrite.onFileChanged(url => {
  picture.src = url
  askForDetails()
})

picture.alt = ferrite.fileName ?? ''
picture.src = ferrite.fileUrl
askForDetails()
layout()
