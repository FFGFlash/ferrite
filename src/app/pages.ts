import { Blocks, Settings, type LucideIcon } from 'lucide-react'
import { createElement, type ComponentType } from 'react'
import type { ExtensionStatus } from '../ipc/extensions'
import { ExtensionPage } from '../panels/ExtensionPage'
import { SettingsPage } from '../panels/SettingsPage'

/**
 * A built-in page: a tab of the workbench's own, such as the settings
 * editor. Pages open where documents do (`page:<id>` in the dock).
 */
export interface PageDescriptor {
  id: string
  title: string
  icon: LucideIcon
  component: ComponentType
}

export const pages: readonly PageDescriptor[] = [
  { id: 'settings', title: 'Settings', icon: Settings, component: SettingsPage },
]

const EXTENSION_PAGE = 'extension:'

/** Each extension page's component, made once so its tab's content isn't remounted. */
const extensionComponents = new Map<string, ComponentType>()

/**
 * The page `id` names: a built-in page, or an extension's page
 * (`extension:<extension id>`) while that extension is loaded.
 */
export function findPage(
  id: string,
  extensions: readonly ExtensionStatus[],
): PageDescriptor | null {
  const builtin = pages.find(page => page.id === id)
  if (builtin || !id.startsWith(EXTENSION_PAGE)) {
    return builtin ?? null
  }
  const extensionId = id.slice(EXTENSION_PAGE.length)
  const extension = extensions.find(candidate => candidate.id === extensionId)
  if (!extension) {
    return null
  }
  let component = extensionComponents.get(extensionId)
  if (!component) {
    component = () => createElement(ExtensionPage, { id: extensionId })
    extensionComponents.set(extensionId, component)
  }
  return { id, title: `Extension: ${extension.name}`, icon: Blocks, component }
}
