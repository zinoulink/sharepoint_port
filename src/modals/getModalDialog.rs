use std::collections::HashMap;


pub fn get_modal_dialog(id: &str) -> Option<HashMap<String, Box<dyn std::any::Any>>> {
    // Placeholder for window.top._SP_MODALDIALOG
    // In a real Rust application, you'd need to handle this differently
    let sp_modal_dialog: Option<Vec<HashMap<String, Box<dyn std::any::Any>>>> = None;

    if let Some(md) = sp_modal_dialog {
        let sanitized_id = id.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>();

        for modal in md {
            if let Some(modal_id) = modal.get("id") {
                if let Some(modal_id_str) = modal_id.downcast_ref::<String>() {
                    if modal_id_str == &format!("sp_frame_{}", sanitized_id) {
                        return Some(modal.clone());
                    }
                }
            }
        }
    }

    None
}