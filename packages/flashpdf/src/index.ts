import { loadBackend } from "#backend";

import { createRenderer } from "./render.js";

export type { BarcodeProps, Edges, Element, Length, QRCodeProps, Style, Width } from "./element.js";
export { Barcode, QRCode } from "./barcode.js";
export { stylesheet } from "./css.js";
export { createElement } from "./jsx-runtime.js";
export { PageNumber, TotalPages } from "./page.js";
export type { Attachment, EmbeddedFont, FacturX, PdfMetadata, RenderOptions } from "./render.js";

export const render = createRenderer(loadBackend);
