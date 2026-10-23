import { useMemo, useState, type KeyboardEvent } from "react";
import { executeCommand, primaryKeybinding, getCommand } from "../commands/registry";
import type { DirectoryEntry } from "../ipc/workspace";
import { documentsStore, openPath } from "../state/documents";
import { layoutStore } from "../state/layout";
import { useStore } from "../state/store";
import { openFolder, toggleFolder, workspaceStore, type Listing } from "../state/workspace";
import "./ExplorerPanel.css";

interface Row {
  entry: DirectoryEntry;
  depth: number;
  expanded: boolean;
}

/** Folders and files of the open workspace, loaded as folders expand. */
export function ExplorerPanel() {
  const { info, listings } = useStore(workspaceStore);
  const { expanded } = useStore(layoutStore);
  const { documents, activeId } = useStore(documentsStore);
  const [focused, setFocused] = useState<string | null>(null);
  const activePath = documents.find((document) => document.id === activeId)?.path ?? null;

  const rows = useMemo(
    () => (info ? flatten(info.root, listings, new Set(expanded), 0) : []),
    [info, listings, expanded],
  );

  if (!info) {
    return (
      <div className="panel-empty">
        <p>No folder open.</p>
        <button type="button" className="panel-button" onClick={() => void openFolder()}>
          Open Folder…
        </button>
      </div>
    );
  }
  const rootListing = listings.get(info.root);
  if (rootListing && "error" in rootListing) {
    return (
      <p className="panel-empty">
        Could not read {info.name}: {rootListing.error}
      </p>
    );
  }

  const focusIndex = Math.max(
    0,
    rows.findIndex((row) => row.entry.path === focused),
  );
  const activate = (row: Row) => {
    setFocused(row.entry.path);
    if (row.entry.directory) {
      toggleFolder(row.entry.path);
    } else {
      void openPath(row.entry.path);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const row = rows[focusIndex];
    if (!row) {
      return;
    }
    const move = (index: number) => {
      const target = rows[Math.max(0, Math.min(rows.length - 1, index))];
      if (target) {
        setFocused(target.entry.path);
        document.getElementById(rowId(target.entry.path))?.scrollIntoView({ block: "nearest" });
      }
    };
    switch (event.key) {
      case "ArrowDown":
        move(focusIndex + 1);
        break;
      case "ArrowUp":
        move(focusIndex - 1);
        break;
      case "Home":
        move(0);
        break;
      case "End":
        move(rows.length - 1);
        break;
      case "ArrowRight":
        if (row.entry.directory && !row.expanded) {
          toggleFolder(row.entry.path);
        } else {
          move(focusIndex + 1);
        }
        break;
      case "ArrowLeft":
        if (row.entry.directory && row.expanded) {
          toggleFolder(row.entry.path);
        } else {
          const parent = rows
            .slice(0, focusIndex)
            .reverse()
            .find((candidate) => candidate.depth < row.depth);
          if (parent) {
            move(rows.indexOf(parent));
          }
        }
        break;
      case "Enter":
      case " ":
        activate(row);
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  return (
    <ul
      className="explorer-tree"
      role="tree"
      aria-label={info.name}
      tabIndex={0}
      aria-activedescendant={rows[focusIndex] ? rowId(rows[focusIndex].entry.path) : undefined}
      onKeyDown={onKeyDown}
    >
      {rows.map((row, index) => (
        <li
          key={row.entry.path}
          id={rowId(row.entry.path)}
          className="explorer-tree__row"
          role="treeitem"
          aria-level={row.depth + 1}
          aria-expanded={row.entry.directory ? row.expanded : undefined}
          aria-selected={row.entry.path === activePath}
          data-focused={index === focusIndex || undefined}
          data-ignored={row.entry.ignored || undefined}
          style={{ paddingLeft: `calc(${row.depth} * var(--space-3) + var(--space-2))` }}
          title={row.entry.path}
          onClick={() => activate(row)}
        >
          <span className="explorer-tree__chevron" aria-hidden="true">
            {row.entry.directory ? (row.expanded ? "▾" : "▸") : ""}
          </span>
          <span className="explorer-tree__name">{row.entry.name}</span>
        </li>
      ))}
      {rows.length === 0 && (
        <li className="panel-empty" role="none">
          {rootListing ? "This folder is empty." : "Loading…"}
        </li>
      )}
    </ul>
  );
}

/** Explorer actions shown in the side bar header. */
export function ExplorerActions() {
  const { info } = useStore(workspaceStore);
  const collapse = getCommand("explorer.collapseAll");
  const open = getCommand("file.openFolder");
  return (
    <>
      <button
        type="button"
        className="sidebar__action"
        title={`Open Folder…${primaryKeybinding(open) ? ` (${primaryKeybinding(open)})` : ""}`}
        aria-label="Open Folder"
        onClick={() => void executeCommand("file.openFolder")}
      >
        ⊕
      </button>
      {info && collapse && (
        <button
          type="button"
          className="sidebar__action"
          title="Collapse Folders"
          aria-label="Collapse Folders"
          onClick={() => void executeCommand("explorer.collapseAll")}
        >
          ⊟
        </button>
      )}
    </>
  );
}

function flatten(
  directory: string,
  listings: ReadonlyMap<string, Listing>,
  expanded: ReadonlySet<string>,
  depth: number,
): Row[] {
  const listing = listings.get(directory);
  if (!listing || "error" in listing) {
    return [];
  }
  return listing.flatMap((entry) => {
    const isOpen = entry.directory && expanded.has(entry.path);
    const row = { entry, depth, expanded: isOpen };
    return isOpen ? [row, ...flatten(entry.path, listings, expanded, depth + 1)] : [row];
  });
}

function rowId(path: string): string {
  return `explorer-${encodeURIComponent(path)}`;
}
