use serde::de::DeserializeOwned;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["__TAURI__", "core"], js_name = "invoke", catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["__TAURI__", "event"], js_name = "listen", catch)]
    async fn tauri_listen(
        event: &str,
        handler: &Closure<dyn FnMut(JsValue)>,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["__TAURI__", "core"], js_name = "convertFileSrc")]
    fn tauri_convert_file_src(path: &str) -> String;
}

pub fn asset_url(path: &str) -> String {
    tauri_convert_file_src(&path.replace('\\', "/"))
}

pub async fn invoke<A: serde::Serialize, R: DeserializeOwned>(
    cmd: &str,
    args: A,
) -> Result<R, String> {
    let args = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;
    let value = tauri_invoke(cmd, args).await.map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}

pub async fn invoke_no_args<R: DeserializeOwned>(cmd: &str) -> Result<R, String> {
    let value = tauri_invoke(cmd, js_sys::Object::new().into())
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}

pub async fn listen<T: DeserializeOwned, F: FnMut(T) + 'static>(
    event: &str,
    mut cb: F,
) -> Result<(), String> {
    let closure = Closure::new(move |v: JsValue| {
        if let Ok(payload) = js_sys::Reflect::get(&v, &JsValue::from_str("payload")) {
            if let Ok(t) = serde_wasm_bindgen::from_value::<T>(payload) {
                cb(t);
            }
        }
    });
    tauri_listen(event, &closure).await.map_err(js_error)?;
    closure.forget();
    Ok(())
}

fn js_error(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}