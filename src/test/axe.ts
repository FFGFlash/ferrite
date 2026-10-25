import axe from "axe-core";

/**
 * Accessibility violations axe finds in `element`, as readable lines. jsdom
 * has no layout, so checks that need rendering (color contrast) are off;
 * contrast is covered by the theme tokens and checked by hand.
 */
export async function axeViolations(
  element: Element = document.body,
  ignore: readonly string[] = [],
): Promise<string[]> {
  const rules: Record<string, { enabled: boolean }> = { "color-contrast": { enabled: false } };
  for (const rule of ignore) {
    rules[rule] = { enabled: false };
  }
  const results = await axe.run(element, { rules });
  return results.violations.flatMap((violation) =>
    violation.nodes.map((node) => `${violation.id}: ${violation.help} (${node.target.join(" ")})`),
  );
}
