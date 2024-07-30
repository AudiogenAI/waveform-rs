use wasm_bindgen::prelude::*;
mod processor;

#[wasm_bindgen]
pub fn audio_to_waveform(data: Vec<u8>) -> Result<Vec<f32>, JsValue> {
    processor::audio_to_waveform(data).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen(start)]
pub fn main_js() -> Result<(), JsValue> {
    #[cfg(debug_assertions)]
    console_error_panic_hook::set_once();
    Ok(())
}
