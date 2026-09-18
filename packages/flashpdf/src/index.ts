import { createRenderer } from "./render.js";
import { loadWasm } from "./wasm.js";

export type { Element, Length, Style, Width } from "./element.js";
export { stylesheet } from "./css.js";
export { PageNumber, TotalPages } from "./page.js";
export type { EmbeddedFont, RenderOptions } from "./render.js";

export const render = createRenderer(loadWasm);
