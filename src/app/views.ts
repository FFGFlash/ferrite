import { createElement, useMemo, type ComponentType } from 'react'
import type { IconName } from '../components/Icon'
import type { ViewContainerContribution, ViewContribution } from '../ipc/extensions'
import {
  ExtensionContainerPanel,
  ExtensionViewActions,
  ExtensionViewPanel,
} from '../panels/ExtensionViewPanel'
import { extensionsStore, usableExtensions } from '../state/extensions'
import { useStore } from '../state/store'
import { ExplorerActions, ExplorerPanel } from '../panels/ExplorerPanel'
import { ExtensionsActions, ExtensionsPanel } from '../panels/ExtensionsPanel'
import { OutputActions, OutputPanel } from '../panels/OutputPanel'
import { ProblemsActions, ProblemsPanel } from '../panels/ProblemsPanel'
import { SearchPanel } from '../panels/SearchPanel'

/**
 * A side bar view. Views are described declaratively so that the activity bar
 * and side bar render from data, the same shape host-managed extension
 * contributions will use.
 */
export interface ViewDescriptor {
  id: string
  title: string
  icon: IconName
  component: ComponentType
  /** Buttons shown in the side bar header while the view is active. */
  actions?: ComponentType
  /** Command that shows this view, for the activity bar tooltip. */
  command?: string
}

export const builtinViews: readonly ViewDescriptor[] = [
  {
    id: 'explorer',
    title: 'Explorer',
    icon: 'files',
    component: ExplorerPanel,
    actions: ExplorerActions,
    command: 'workbench.showExplorer',
  },
  {
    id: 'search',
    title: 'Search',
    icon: 'search',
    component: SearchPanel,
    command: 'workbench.showSearch',
  },
  {
    id: 'extensions',
    title: 'Extensions',
    icon: 'extensions',
    component: ExtensionsPanel,
    actions: ExtensionsActions,
    command: 'workbench.showExtensions',
  },
  // Panel views: they open along the bottom (see PANEL_VIEWS in the dock).
  {
    id: 'problems',
    title: 'Problems',
    icon: 'problems',
    component: ProblemsPanel,
    actions: ProblemsActions,
    command: 'workbench.showProblems',
  },
  {
    id: 'output',
    title: 'Output',
    icon: 'output',
    component: OutputPanel,
    actions: OutputActions,
    command: 'workbench.showOutput',
  },
]

/** Components per contributed view and container, kept so React doesn't remount them. */
const contributedComponents = new Map<
  string,
  { component: ComponentType; actions?: ComponentType }
>()

function contributedView(view: ViewContribution): ViewDescriptor {
  let components = contributedComponents.get(view.fullId)
  if (!components) {
    components = {
      component: () => createElement(ExtensionViewPanel, { view: view.fullId }),
      actions: () => createElement(ExtensionViewActions, { view: view.fullId }),
    }
    contributedComponents.set(view.fullId, components)
  }
  return { id: view.fullId, title: view.title, icon: view.icon as IconName, ...components }
}

/** A container: its views are sections, each with its own header and buttons. */
function contributedContainer(container: ViewContainerContribution): ViewDescriptor {
  let components = contributedComponents.get(container.fullId)
  if (!components) {
    components = {
      component: () => createElement(ExtensionContainerPanel, { container: container.fullId }),
    }
    contributedComponents.set(container.fullId, components)
  }
  return {
    id: container.fullId,
    title: container.title,
    icon: container.icon as IconName,
    ...components,
  }
}

/**
 * Built-in views followed by those of enabled extensions: their own views,
 * and their containers (whose views are sections of them).
 */
export function listViews(extensions = extensionsStore.get()): readonly ViewDescriptor[] {
  return [
    ...builtinViews,
    ...usableExtensions(extensions).flatMap(extension => [
      ...extension.contributions.viewContainers.map(contributedContainer),
      ...extension.contributions.views.filter(view => view.container === null).map(contributedView),
    ]),
  ]
}

export function useViews(): readonly ViewDescriptor[] {
  const extensions = useStore(extensionsStore)
  return useMemo(() => listViews(extensions), [extensions])
}
