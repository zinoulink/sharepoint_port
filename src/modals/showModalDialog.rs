use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{window, Document, Element, HtmlElement};
use js_sys::Promise;

#[wasm_bindgen]
pub struct ModalDialog {
    id: String,
    options: ModalOptions,
}

#[derive(Default)]
pub struct ModalOptions {
    id: Option<String>,
    title: Option<String>,
    message: Option<String>,
    html: Option<String>,
    width: Option<String>,
    height: Option<String>,
    wait: bool,
    close_previous: bool,
    show_close: bool,
    hide_close: bool,
    url: Option<String>,
    on_load: Option<js_sys::Function>,
    on_url_load: Option<js_sys::Function>,
}

#[wasm_bindgen]
impl ModalDialog {
    #[wasm_bindgen(constructor)]
    pub fn new(options: JsValue) -> Result<ModalDialog, JsValue> {
        let options: ModalOptions = options.into_serde().map_err(|_| "Invalid options")?;
        let id = options.id.clone().unwrap_or_else(|| js_sys::Date::now().to_string());
        Ok(ModalDialog { id, options })
    }

    pub fn show(&self) -> Promise {
        let id = self.id.clone();
        let options = self.options.clone();

        future_to_promise(async move {
            if !is_sp_modal_dialog_loaded() {
                load_sp_ui_dialog_js().await?;
            }

            let modal_id = format!("sp_frame_{}", id);
            let document = window().unwrap().document().unwrap();

            if let Some(html) = &options.html {
                create_html_content(&document, &modal_id, html);
            }

            adjust_size(&mut options);

            if options.close_previous {
                close_previous_dialog();
            }

            let modal = show_modal(&options);
            setup_modal_frame(&document, &modal_id, &options);

            if let Some(on_load) = options.on_load {
                on_load.call0(&JsValue::NULL)?;
            }

            if let Some(url) = &options.url {
                if let Some(on_url_load) = options.on_url_load {
                    setup_iframe_ready(&document, &modal_id, url, on_url_load);
                }
            }

            Ok(JsValue::NULL)
        })
    }
}

fn is_sp_modal_dialog_loaded() -> bool {
    // This would need to be implemented to check if SP.UI.ModalDialog is available
    false
}

async fn load_sp_ui_dialog_js() -> Result<(), JsValue> {
    // This would need to be implemented to load the SP UI Dialog JS file
    Ok(())
}

fn create_html_content(document: &Document, modal_id: &str, html: &str) {
    let div = document.create_element("div").unwrap();
    div.set_attribute("style", "padding:10px;display:inline-block").unwrap();
    div.set_class_name("sp-showModalDialog");
    div.set_id(&format!("content_{}", modal_id));
    div.set_inner_html(html);
    // You would need to add this div to the document or the modal
}

fn adjust_size(options: &mut ModalOptions) {
    // This function would adjust the width and height based on the viewport size
    // Similar to the JavaScript version, but using Rust's logic
}

fn close_previous_dialog() {
    // This function would close the previous dialog
    // You'd need to implement this based on how you're tracking dialogs
}

fn show_modal(options: &ModalOptions) -> JsValue {
    // This function would show the modal dialog
    // You'd need to call into JavaScript to use SP.UI.ModalDialog.showModalDialog
    JsValue::NULL
}

fn setup_modal_frame(document: &Document, modal_id: &str, options: &ModalOptions) {
    // This function would set up the modal frame, including z-index handling
    // You'd need to implement this based on your specific requirements
}

fn setup_iframe_ready(document: &Document, modal_id: &str, url: &str, on_url_load: js_sys::Function) {
    // This function would set up the iframe ready event
    // You'd need to implement this based on your specific requirements
}

fn future_to_promise<F>(future: F) -> Promise
where
    F: Future<Output = Result<JsValue, JsValue>> + 'static,
{
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        spawn_local(async move {
            match future.await {
                Ok(value) => resolve.call1(&JsValue::NULL, &value).unwrap(),
                Err(error) => reject.call1(&JsValue::NULL, &error).unwrap(),
            }
        });
    });
    promise
}