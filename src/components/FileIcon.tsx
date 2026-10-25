import type { EntryIcon } from './fileIcons'

interface FileIconProps {
  icon: EntryIcon
  kind: 'file' | 'folder'
  className: string
}

/** A file or folder icon: an image from the icon theme, or a built-in icon. Decorative. */
export function FileIcon({ icon, kind, className }: FileIconProps) {
  if ('src' in icon) {
    return (
      <img
        className={className}
        data-kind={kind}
        src={icon.src}
        alt=""
        width={16}
        height={16}
        draggable={false}
      />
    )
  }
  const { Icon } = icon
  return (
    <Icon className={className} data-kind={kind} size={16} strokeWidth={1.5} aria-hidden="true" />
  )
}
