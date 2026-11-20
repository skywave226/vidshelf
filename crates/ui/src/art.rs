use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, HtmlScriptElement};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["__vmArt"], js_name = "create")]
    pub fn art_create(container: &Element, url: &str, video_id: i64, on_error: &Closure<dyn FnMut()>);

    #[wasm_bindgen(js_namespace = ["__vmArt"], js_name = "destroy")]
    pub fn art_destroy(container: &Element);
}

async fn load_script(doc: &Document, url: &str) -> Result<(), String> {
    let script = doc
        .create_element("script")
        .map_err(|_| "无法创建 script".to_string())?
        .dyn_into::<HtmlScriptElement>()
        .map_err(|_| "script 类型错误".to_string())?;
    script.set_src(url);
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve = resolve.dyn_into::<js_sys::Function>().unwrap();
        let reject = reject.dyn_into::<js_sys::Function>().unwrap();
        let onload = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let onerror = Closure::once_into_js(move || {
            let _ = reject.call0(&JsValue::NULL);
        });
        script.set_onload(Some(onload.as_ref().unchecked_ref()));
        script.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    });
    doc.head()
        .ok_or("无 head")?
        .append_child(&script)
        .map_err(|_| "无法挂载 script".to_string())?;
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|_| format!("脚本加载失败：{url}"))?;
    Ok(())
}

// 幂等注入本地 ArtPlayer + 胶水脚本（运行期离线，资源由 dx 打包）
pub async fn ensure_loaded(art_url: &str, glue_url: &str) -> Result<(), String> {
    let win = web_sys::window().ok_or("无 window")?;
    if let Ok(v) = js_sys::Reflect::get(&win, &JsValue::from_str("__vmArt")) {
        if !v.is_undefined() && !v.is_null() {
            return Ok(());
        }
    }
    let doc = win.document().ok_or("无 document")?;
    load_script(&doc, art_url).await?;
    load_script(&doc, glue_url).await?;
    Ok(())
}