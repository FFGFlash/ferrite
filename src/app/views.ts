import type { ComponentType } from "react";
import type { IconName } from "../components/Icon";
import { ExplorerPanel } from "../panels/ExplorerPanel";
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
}

export const builtinViews: readonly ViewDescriptor[] = [
  { id: "explorer", title: "Explorer", icon: "files", component: ExplorerPanel },
  { id: "search", title: "Search", icon: "search", component: SearchPanel },
];
