const paths = {
  files:
    "M14 3H7a2 2 0 0 0-2 2v12M9 7h6l4 4v8a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2ZM15 7v4h4",
  search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13ZM15.5 15.5 20 20",
  extensions: "M4 4h6v6H4ZM14 4h6v6h-6ZM4 14h6v6H4ZM17 14v6M14 17h6",
} as const;

export type IconName = keyof typeof paths;

interface IconProps {
  name: IconName;
  size?: number;
}

/** Stroke icon drawn in the current text color. Decorative; label the control instead. */
export function Icon({ name, size = 24 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={paths[name]} />
    </svg>
  );
}
