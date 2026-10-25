/** Which icon the explorer shows for a file, from its name. */
import {
  File,
  FileArchive,
  FileBox,
  FileBraces,
  FileCode,
  FileCog,
  FileImage,
  FileKey,
  FileLock,
  FileMusic,
  FileSpreadsheet,
  FileTerminal,
  FileText,
  FileType,
  FileVideoCamera,
  type LucideIcon,
} from 'lucide-react'

const byExtension = new Map<string, LucideIcon>()
const byName = new Map<string, LucideIcon>()

function add(table: Map<string, LucideIcon>, icon: LucideIcon, keys: string): void {
  for (const key of keys.split(' ')) {
    table.set(key, icon)
  }
}

add(
  byExtension,
  FileCode,
  'rs ts tsx mts cts js jsx mjs cjs py go c h cc cpp cxx hpp hh cs java kt kts swift rb php ' +
    'lua zig hs ml scala dart ex exs erl clj vue svelte astro html htm css scss sass less wit wat sql',
)
add(byExtension, FileBraces, 'json jsonc json5')
add(byExtension, FileCog, 'toml yaml yml ini cfg conf properties env xml')
add(byExtension, FileTerminal, 'sh bash zsh fish ps1 psm1 bat cmd')
add(byExtension, FileText, 'md mdx markdown txt rst adoc org log')
add(byExtension, FileImage, 'png jpg jpeg gif svg webp avif bmp ico icns tif tiff')
add(byExtension, FileArchive, 'zip tar gz tgz xz bz2 zst 7z rar')
add(byExtension, FileLock, 'lock')
add(byExtension, FileMusic, 'mp3 wav ogg flac m4a')
add(byExtension, FileVideoCamera, 'mp4 mov webm mkv avi')
add(byExtension, FileSpreadsheet, 'csv tsv xls xlsx ods')
add(byExtension, FileType, 'ttf otf woff woff2')
add(byExtension, FileBox, 'wasm exe dll so dylib bin')
add(byExtension, FileKey, 'pem key crt cer p12')

add(
  byName,
  FileCog,
  '.gitignore .gitattributes .gitmodules .editorconfig .prettierrc .prettierignore .npmrc ' +
    '.env dockerfile makefile justfile',
)
add(byName, FileLock, 'package-lock.json pnpm-lock.yaml yarn.lock')
add(byName, FileText, 'readme license licence changelog authors contributing')

/** The icon for a file called `name`. */
export function fileIcon(name: string): LucideIcon {
  const lower = name.toLowerCase()
  const dot = lower.lastIndexOf('.')
  const stem = dot > 0 ? lower.slice(0, dot) : lower
  return (
    byName.get(lower) ??
    (dot > 0 ? byExtension.get(lower.slice(dot + 1)) : undefined) ??
    // LICENSE-MIT, CHANGELOG.old, …
    byName.get(stem.split(/[-_]/)[0] ?? stem) ??
    File
  )
}
