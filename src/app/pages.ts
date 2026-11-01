import { Settings, type LucideIcon } from 'lucide-react'
import type { ComponentType } from 'react'
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
