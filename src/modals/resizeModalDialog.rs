use web_sys::{window, Element, HtmlElement};
use wasm_bindgen::JsCast;

struct DialogElements {
    border: Element,
    title_text: Element,
    content: Element,
    frame: Element,
}

struct ResizeOptions {
    id: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
}

pub fn resize_modal_dialog(options: ResizeOptions) {
    let dlg = find_modal_dialog(options.id);
    if let Some(dlg) = dlg {
        let elements = get_dialog_elements(&dlg);
        
        let width = options.width.unwrap_or_else(|| parse_int_style(&dlg, "width"));
        let height = options.height.unwrap_or_else(|| parse_int_style(&dlg, "height"));
        
        let delta_width = width - elements.border.client_width();
        let delta_height = height - elements.border.client_height();
        
        update_element_sizes(&elements, delta_width, delta_height);
        center_dialog(&dlg);
    }
}

fn find_modal_dialog(id: Option<String>) -> Option<Element> {
    let window = window().unwrap();
    let document = window.document().unwrap();
    
    if let Some(id) = id {
        document.get_element_by_id(&format!("sp_frame_{}", id))
    } else {
        // Assuming we have a way to get the last modal dialog ID
        // This part would need to be implemented based on how you're tracking modal dialogs
        None
    }
}

fn get_dialog_elements(dlg: &Element) -> DialogElements {
    DialogElements {
        border: dlg.query_selector(".ms-dlgBorder").unwrap().unwrap(),
        title_text: dlg.query_selector(".ms-dlgTitleText").unwrap().unwrap(),
        content: dlg.clone(),
        frame: dlg.query_selector(".ms-dlgFrame").unwrap().unwrap(),
    }
}

fn update_element_sizes(elements: &DialogElements, delta_width: i32, delta_height: i32) {
    let update = |el: &Element, adjust_height: bool| {
        let html_el = el.dyn_ref::<HtmlElement>().unwrap();
        html_el.style().set_property("width", &format!("{}px", html_el.offset_width() + delta_width)).unwrap();
        if adjust_height {
            html_el.style().set_property("height", &format!("{}px", html_el.offset_height() + delta_height)).unwrap();
        }
    };

    update(&elements.border, true);
    update(&elements.title_text, false);
    update(&elements.content, true);
    update(&elements.frame, true);
}

fn center_dialog(dlg: &Element) {
    let window = window().unwrap();
    let html_dlg = dlg.dyn_ref::<HtmlElement>().unwrap();
    
    let page_height = window.inner_height().unwrap().as_f64().unwrap() as i32;
    let page_width = window.inner_width().unwrap().as_f64().unwrap() as i32;
    
    let top = (page_height / 2) - (html_dlg.offset_height() / 2);
    let left = (page_width / 2) - (html_dlg.offset_width() / 2);
    
    html_dlg.style().set_property("top", &format!("{}px", top)).unwrap();
    html_dlg.style().set_property("left", &format!("{}px", left)).unwrap();
}

fn parse_int_style(el: &Element, prop: &str) -> i32 {
    let html_el = el.dyn_ref::<HtmlElement>().unwrap();
    html_el.style().get_property_value(prop)
        .unwrap_or_default()
        .parse()
        .unwrap_or(0)
}