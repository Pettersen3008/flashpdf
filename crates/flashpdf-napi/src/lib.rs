//! N-API transport for the shared command protocol.
//!
//! Same decoder, same validation, same bounded input window as the WASM
//! adapter. The only difference is how bytes arrive: Node hands over a borrowed
//! `Buffer` per synchronous `push` instead of writing into linear memory.

use flashpdf_core::protocol::{Decoder, INPUT_CAPACITY};
use napi::bindgen_prelude::{Buffer, BufferSlice, Result};
use napi_derive::napi;

#[napi]
pub struct PdfRenderer {
    input: Box<[u8]>,
    decoder: Decoder,
}

#[napi]
impl PdfRenderer {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            input: vec![0; INPUT_CAPACITY].into_boxed_slice(),
            decoder: Decoder::default(),
        }
    }

    #[napi]
    pub fn input_capacity(&self) -> u32 {
        self.input.len() as u32
    }

    /// N-API only borrows a Buffer for this call. The decoder takes the single
    /// Vec it needs to keep until it writes the PDF's embedded font stream.
    #[napi]
    pub fn add_font(&mut self, bytes: BufferSlice<'_>) -> Result<u32> {
        self.decoder
            .add_font(bytes.to_vec())
            .map(u32::from)
            .map_err(error)
    }

    /// `chunk` borrows JS-owned memory that is only valid for this call, so the
    /// bytes land in Rust-owned scratch before the decoder sees them.
    #[napi]
    pub fn push(&mut self, chunk: BufferSlice<'_>) -> Result<()> {
        let length = chunk.len();
        if length > self.input.len() {
            return Err(error("input exceeds window"));
        }
        self.input[..length].copy_from_slice(&chunk);
        self.decoder.push(&self.input[..length]).map_err(error)
    }

    #[napi]
    pub fn finish(&mut self) -> Result<Buffer> {
        std::mem::take(&mut self.decoder)
            .finish()
            .map(Buffer::from)
            .map_err(error)
    }
}

impl Default for PdfRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn error(message: impl Into<String>) -> napi::Error {
    napi::Error::from_reason(message.into())
}
