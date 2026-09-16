use wasm_bindgen::prelude::*;

use flashpdf_core::protocol::{Decoder, INPUT_CAPACITY};

#[wasm_bindgen]
pub struct PdfRenderer {
    input: Box<[u8]>,
    decoder: Decoder,
}

#[wasm_bindgen]
impl PdfRenderer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            input: vec![0; INPUT_CAPACITY].into_boxed_slice(),
            decoder: Decoder::default(),
        }
    }

    pub fn input_ptr(&mut self) -> usize {
        self.input.as_mut_ptr() as usize
    }

    pub fn input_capacity(&self) -> usize {
        self.input.len()
    }

    pub fn add_font(&mut self, bytes: Vec<u8>) -> Result<u8, JsValue> {
        self.decoder
            .add_font(bytes)
            .map_err(|error| JsValue::from_str(&error))
    }

    pub fn push(&mut self, length: usize) -> Result<(), JsValue> {
        if length > self.input.len() {
            return Err(JsValue::from_str("input exceeds window"));
        }
        self.decoder
            .push(&self.input[..length])
            .map_err(|error| JsValue::from_str(&error))
    }

    pub fn finish(self) -> Result<Vec<u8>, JsValue> {
        self.decoder
            .finish()
            .map_err(|error| JsValue::from_str(&error))
    }
}

impl Default for PdfRenderer {
    fn default() -> Self {
        Self::new()
    }
}
