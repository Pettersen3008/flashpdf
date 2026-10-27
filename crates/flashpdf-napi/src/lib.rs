//! N-API adapter with the same surface as the WASM one. Native code cannot hand
//! JS its own memory safely, so JS owns the input window and lends it to each
//! `push_bytes` call, which decodes it in place. Holding the buffer across calls
//! would let JS detach it under a live pointer.

use flashpdf_core::protocol::{Decoder, INPUT_CAPACITY as CAPACITY};
use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;

#[napi]
pub const INPUT_CAPACITY: u32 = CAPACITY as u32;

#[napi]
#[derive(Default)]
pub struct PdfRenderer {
    decoder: Decoder,
}

#[napi]
impl PdfRenderer {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// The window starts the buffer JS passes as memory.
    #[napi(js_name = "input_ptr")]
    pub fn input_ptr(&self) -> u32 {
        0
    }

    #[napi(js_name = "input_capacity")]
    pub fn input_capacity(&self) -> u32 {
        INPUT_CAPACITY
    }

    #[napi(js_name = "add_font")]
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<u32> {
        self.decoder
            .add_font(bytes.to_vec())
            .map(u32::from)
            .map_err(|value| error(value.to_string()))
    }

    #[napi(js_name = "stage_image")]
    pub fn stage_image(&mut self, bytes: &[u8]) -> Result<u32> {
        self.decoder
            .stage_image(bytes.to_vec())
            .map(u32::from)
            .map_err(|value| error(value.to_string()))
    }

    #[napi(js_name = "add_attachment")]
    pub fn add_attachment(&mut self, bytes: &[u8]) -> Result<u32> {
        self.decoder
            .add_attachment(bytes.to_vec())
            .map(u32::from)
            .map_err(|value| error(value.to_string()))
    }

    /// `input` is borrowed for this call only.
    #[napi(js_name = "push_bytes")]
    pub fn push_bytes(&mut self, input: &[u8]) -> Result<()> {
        if input.len() > CAPACITY {
            return Err(error("input exceeds window"));
        }
        self.decoder
            .push(input)
            .map_err(|value| error(value.to_string()))
    }

    #[napi]
    pub fn finish(&mut self) -> Result<Uint8Array> {
        std::mem::take(&mut self.decoder)
            .finish()
            .map(Uint8Array::from)
            .map_err(|value| error(value.to_string()))
    }

    /// Drops the decoder's fonts and images now instead of at garbage collection.
    #[napi]
    pub fn free(&mut self) {
        self.decoder = Decoder::default();
    }
}

fn error(message: impl Into<String>) -> napi::Error {
    napi::Error::from_reason(message.into())
}
