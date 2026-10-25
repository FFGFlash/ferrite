import { createElement, useMemo, type ComponentType } from "react";
import type { IconName } from "../components/Icon";
import type { ViewContribution } from "../ipc/extensions";
import { ExtensionViewPanel } from "../panels/ExtensionViewPanel";
import { extensionsStore, usableExtensions } from "../state/extensions";
import { useStore } from "../state/store";
import { ExplorerActions, ExplorerPanel } from "../panels/ExplorerPanel";
import { ExtensionsActions, ExtensionsPanel } from "../panels/ExtensionsPanel";
import { SearchPanel } from "../panels/SearchPanel";

/**
 * A side bar view. Views are described declaratively so that the activity bar
 * and side bar render from data, the same shape host-managed extension
 * contributions will use.
 */
export interface ViewDescriptor {
  id: string;
  title: string;
  icon: IconName;
  component: ComponentType;
  /** Buttons shown in the side bar header while the view is active. */
  actions?: ComponentType;
  /** Command that shows this view, for the activity bar tooltip. */
  command?: string;
}

export const builtinViews: readonly ViewDescriptor[] = [
  {
    id: "explorer",
    title: "Explorer",
    icon: "files",
    component: ExplorerPanel,
    actions: ExplorerActions,
    command: "workbench.showExplorer",
  },
  {
    id: "search",
    title: "Search",
    icon: "search",
    component: SearchPanel,
    command: "workbench.showSearch",
  },
  {
    id: "extensions",
    title: "Extensions",
    icon: "extensions",
    component: ExtensionsPanel,
    actions: ExtensionsActions,
    command: "workbench.showExtensions",
  },
];

/** One component per contributed view, kept so React doesn't remount it. */
const contributedComponents = new Map<string, ComponentType>();

function contributedView(view: ViewContribution): ViewDescriptor {
  let component = contributedComponents.get(view.fullId);
  if (!component) {
    component = () => createElement(ExtensionViewPanel, { view: view.fullId });
    contributedComponents.set(view.fullId, component);
  }
  return { id: view.fullId, title: view.title, icon: view.icon as IconName, component };
}

/** Built-in views followed by those of enabled extensions. */
export function useViews(): readonly ViewDescriptor[] {
  const extensions = useStore(extensionsStore);
  return useMemo(
    () => [
      ...builtinViews,
      ...usableExtensions(extensions).flatMap((extension) =>
        extension.contributions.views.map(contributedView),
      ),
    ],
    [extensions],
  );
}
