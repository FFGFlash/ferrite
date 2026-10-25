import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { getViewContent, viewOpened, type ViewContent, type ViewEntry } from "../ipc/extensions";
import { runContributed, viewVersions } from "../state/extensions";
import { reportError } from "../state/notifications";
import { useStore } from "../state/store";
import "./ExtensionViewPanel.css";

interface Row {
  item: ViewEntry;
  depth: number;
  hasChildren: boolean;
}

/** The rows shown with the items whose ids are in `open` expanded. */
function visibleRows(items: readonly ViewEntry[], open: ReadonlySet<string>): Row[] {
  const children = new Map<string | null, ViewEntry[]>();
  for (const item of items) {
    const siblings = children.get(item.parent) ?? [];
    siblings.push(item);
    children.set(item.parent, siblings);
  }
  const rows: Row[] = [];
  const visit = (parent: string | null, depth: number) => {
    for (const item of children.get(parent) ?? []) {
      const hasChildren = children.has(item.id);
      rows.push({ item, depth, hasChildren });
      if (hasChildren && open.has(item.id)) {
        visit(item.id, depth + 1);
      }
    }
  };
  visit(null, 0);
  return rows;
}

/**
 * A side bar view contributed by an extension. The extension sets its items
 * (a tree) and message; activating an item runs the item's command with its
 * argument. The view is rendered by the workbench from that data only.
 */
export function ExtensionViewPanel({ view }: { view: string }) {
  const version = useStore(viewVersions)[view] ?? 0;
  const [content, setContent] = useState<ViewContent | null>(null);
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const [focused, setFocused] = useState(0);
  const seen = useRef(new Set<string>());
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    void viewOpened(view).catch((error: unknown) => reportError("Opening the view", error));
  }, [view]);

  useEffect(() => {
    let cancelled = false;
    getViewContent(view)
      .then((next) => {
        if (cancelled) {
          return;
        }
        // Items appearing for the first time start as the extension asks.
        setOpen((current) => {
          const result = new Set(current);
          for (const item of next.items) {
            if (!seen.current.has(item.id)) {
              seen.current.add(item.id);
              if (item.expanded) {
                result.add(item.id);
              }
            }
          }
          return result;
        });
        setContent(next);
      })
      .catch((error: unknown) => reportError("Loading the view", error));
    return () => {
      cancelled = true;
    };
  }, [view, version]);

  const rows = useMemo(() => visibleRows(content?.items ?? [], open), [content, open]);
  const current = Math.min(focused, rows.length - 1);

  const toggle = (id: string, expand?: boolean) =>
    setOpen((previous) => {
      const next = new Set(previous);
      if (expand ?? !next.has(id)) {
        next.add(id);
      } else {
        next.delete(id);
      }
      return next;
    });

  const activate = (row: Row) => {
    if (row.item.command) {
      runContributed(row.item.command, row.item.argument).catch((error: unknown) =>
        reportError(row.item.label, error),
      );
    } else if (row.hasChildren) {
      toggle(row.item.id);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const row = rows[current];
    if (!row) {
      return;
    }
    const move = (index: number) => {
      event.preventDefault();
      setFocused(Math.max(0, Math.min(rows.length - 1, index)));
    };
    switch (event.key) {
      case "ArrowDown":
        move(current + 1);
        break;
      case "ArrowUp":
        move(current - 1);
        break;
      case "Home":
        move(0);
        break;
      case "End":
        move(rows.length - 1);
        break;
      case "ArrowRight":
        event.preventDefault();
        if (row.hasChildren) {
          toggle(row.item.id, true);
        }
        break;
      case "ArrowLeft":
        event.preventDefault();
        if (row.hasChildren && open.has(row.item.id)) {
          toggle(row.item.id, false);
        } else if (row.item.parent) {
          move(rows.findIndex((candidate) => candidate.item.id === row.item.parent));
        }
        break;
      case "Enter":
        event.preventDefault();
        activate(row);
        break;
    }
  };

  useEffect(() => {
    listRef.current?.children[current]?.scrollIntoView({ block: "nearest" });
  }, [current]);

  return (
    <div className="extension-view">
      {content?.message && <p className="extension-view__message">{content.message}</p>}
      {rows.length > 0 && (
        <ul
          ref={listRef}
          className="extension-view__tree"
          role="tree"
          aria-label="Items"
          tabIndex={0}
          aria-activedescendant={rows[current] ? `${view}-${current}` : undefined}
          onKeyDown={onKeyDown}
        >
          {rows.map((row, index) => (
            <li
              key={row.item.id}
              id={`${view}-${index}`}
              role="treeitem"
              aria-level={row.depth + 1}
              aria-expanded={row.hasChildren ? open.has(row.item.id) : undefined}
              aria-selected={index === current}
              className="extension-view__row"
              data-focused={index === current || undefined}
              style={{ paddingLeft: 8 + row.depth * 12 }}
              title={row.item.tooltip ?? undefined}
              onClick={() => {
                setFocused(index);
                if (row.hasChildren && !row.item.command) {
                  toggle(row.item.id);
                } else {
                  activate(row);
                }
              }}
            >
              <span
                className="extension-view__chevron"
                onClick={(event) => {
                  if (row.hasChildren) {
                    event.stopPropagation();
                    toggle(row.item.id);
                  }
                }}
              >
                {row.hasChildren ? (open.has(row.item.id) ? "▾" : "▸") : ""}
              </span>
              <span className="extension-view__label">{row.item.label}</span>
              {row.item.description && (
                <span className="extension-view__description">{row.item.description}</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
