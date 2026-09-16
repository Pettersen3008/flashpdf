import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { initSync, PdfRenderer } from "../wasm/flashpdf_wasm.js";
import { Binary } from "../dist/binary.js";
import { createNativeRenderer } from "../dist/native.js";

const wasm = initSync({
  module: readFileSync(new URL("../wasm/flashpdf_wasm_bg.wasm", import.meta.url)),
});
const backends = {
  wasm: () => ({ renderer: new PdfRenderer(), memory: wasm.memory }),
  native: () => createNativeRenderer(),
};
const font = readFileSync(new URL("./fixtures/Abel-Regular.ttf", import.meta.url));

// Every opcode the JSX lowering emits, on a 120x60 page that paginates.
const representative = (binary) => {
  binary.header(120, 60, 10);
  const text = (value) =>
    binary.record(1, () => {
      binary.f32(10);
      binary.text(value);
    });
  text("Before");
  binary.record(17, () => {
    for (let index = 0; index < 9; index += 1) binary.f32(0);
    binary.u8(1);
    binary.u8(0xee);
    binary.u8(0xee);
    binary.u8(0xee);
    binary.u8(0);
    binary.u8(0);
    binary.u8(0);
  });
  text("Boxed");
  binary.record(18);
  binary.record(5, () => {
    binary.u16(2);
    binary.u8(0);
    binary.f32(50);
    binary.u8(1);
    binary.f32(1);
  });
  text("row");
  text("1");
  binary.record(6);
  binary.record(2, () => binary.f32(5));
  binary.record(3, () => binary.f32(0));
  text("Stacked");
  binary.record(4);
  binary.record(16, () => {
    binary.f32(10);
    binary.u8(1);
    binary.u8(0xcc);
    binary.u8(0);
    binary.u8(0);
    binary.u8(1);
    binary.text("Styled");
  });
  binary.record(7);
  text("After");
  binary.record(255);
};

function render(backend, stream) {
  const { renderer, memory } = backends[backend]();
  const addFont = renderer.add_font ?? renderer.addFont;
  addFont.call(renderer, font);
  addFont.call(renderer, font);
  stream(new Binary(renderer, memory));
  return Buffer.from(renderer.finish());
}

test("given the same command stream, when rendering, then native output matches WASM byte for byte", () => {
  const expected = render("wasm", representative);
  assert.equal(expected.subarray(0, 5).toString(), "%PDF-");
  assert.deepEqual(render("native", representative), expected, "backends differ");
  assert.deepEqual(render("native", representative), expected, "output is not deterministic");
});

test("given embedded regular and bold fonts, when rendering through both adapters, then bytes match", () => {
  const fonts = (binary) => {
    binary.header(120, 60, 10);
    for (const slot of [2, 3])
      binary.record(16, () => {
        binary.f32(10);
        binary.u8(0);
        binary.u8(0);
        binary.u8(0);
        binary.u8(0);
        binary.u8(slot);
        binary.text("Font");
      });
    binary.record(255);
  };
  const expected = render("wasm", fonts);
  assert.match(expected.toString("latin1"), /\/Subtype \/TrueType/);
  assert.deepEqual(render("native", fonts), expected);
});

test("given malformed input, when decoding, then both backends reject with the same message", () => {
  const cases = {
    "unknown opcode": (binary) => {
      binary.header(120, 60, 10);
      binary.record(42);
    },
    "row cell count mismatch": (binary) => {
      binary.header(120, 60, 10);
      binary.record(5, () => {
        binary.u16(1);
        binary.u8(0);
        binary.f32(50);
      });
      binary.record(1, () => {
        binary.f32(10);
        binary.text("a");
      });
      binary.record(1, () => {
        binary.f32(10);
        binary.text("b");
      });
    },
    "invalid nesting": (binary) => {
      binary.header(120, 60, 10);
      binary.record(18);
    },
    "unsupported WinAnsi character": (binary) => {
      binary.header(120, 60, 10);
      binary.record(1, () => {
        binary.f32(10);
        binary.text("\u{1f642}");
      });
    },
  };
  for (const [expected, stream] of Object.entries(cases)) {
    for (const backend of Object.keys(backends)) {
      const { renderer, memory } = backends[backend]();
      assert.throws(
        () => stream(new Binary(renderer, memory)),
        new RegExp(expected),
        `${backend}: ${expected}`,
      );
    }
  }
});

test("given a push larger than the window, when the addon receives it, then it rejects before decoding", () => {
  const native = new (createRequire(import.meta.url)(
    "../../../dist/napi/flashpdf.node",
  ).PdfRenderer)();
  assert.throws(
    () => native.push(Buffer.alloc(native.inputCapacity() + 1)),
    /input exceeds window/,
  );
});
