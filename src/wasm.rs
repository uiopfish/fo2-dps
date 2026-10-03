use std::cell::RefCell;

use wasm_bindgen::prelude::*;

use crate::web::{WebBundle, WebData, dispatch_json};

thread_local! {
    static DATA: RefCell<Option<WebData>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn initialize(bundle_json: &str) -> Result<(), JsValue> {
    let bundle: WebBundle = serde_json::from_str(bundle_json)
        .map_err(|error| JsValue::from_str(&format!("could not parse web bundle: {error}")))?;
    let data = WebData::from_bundle(bundle).map_err(|error| JsValue::from_str(&error))?;
    DATA.with(|slot| {
        *slot.borrow_mut() = Some(data);
    });
    Ok(())
}

#[wasm_bindgen]
pub fn request(method: &str, target: &str, body_json: &str) -> Result<String, JsValue> {
    DATA.with(|slot| {
        let data = slot.borrow();
        let data = data
            .as_ref()
            .ok_or_else(|| JsValue::from_str("web data has not been initialized"))?;
        dispatch_json(data, method, target, body_json.as_bytes())
            .map_err(|error| JsValue::from_str(&error))
    })
}
