import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// jsdom does not implement layout; scrolling is a no-op in tests.
Element.prototype.scrollIntoView ??= function scrollIntoView() {};

afterEach(() => {
  cleanup();
});
