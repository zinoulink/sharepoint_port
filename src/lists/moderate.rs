use reqwest::Client;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during the moderation process.
#[derive(Debug, Error)]
pub enum ModerateError {
    #[error("[SharepointSharp 'moderate'] The list ID/Name is required.")]
    MissingListId,
    #[error("[SharepointSharp 'moderate'] Not able to find the URL!")]
    MissingSiteUrl,
    #[error("[SharepointSharp 'moderate'] You have to provide the item ID called 'ID'")]
    MissingItemId,
    #[error("[SharepointSharp 'moderate'] You have to provide the approval status 'ApprovalStatus'")]
    MissingApprovalStatus,
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("XML parsing failed: {0}")]
    XmlError(#[from] quick_xml::Error),
}

/// Represents an item to be moderated.
#[derive(Debug, Clone)]
pub struct ModerateItem {
    pub id: u32,
    pub approval_status: String,
    pub other_fields: HashMap<String, String>,
    pub error_message: Option<String>,
}

impl ModerateItem {
    pub fn new(id: u32, approval_status: &str) -> Self {
        Self {
            id,
            approval_status: approval_status.to_string(),
            other_fields: HashMap::new(),
            error_message: None,
        }
    }

    pub fn add_field(&mut self, key: &str, value: &str) {
        self.other_fields.insert(key.to_string(), value.to_string());
    }
}

/// Options for the moderate function.
pub struct ModerateOptions {
    pub packet_size: usize,
    pub progress: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
}

impl Default for ModerateOptions {
    fn default() -> Self {
        Self {
            packet_size: 15,
            progress: None,
        }
    }
}

/// Result of the moderation operation.
#[derive(Debug)]
pub struct ModerateResult {
    pub passed: Vec<HashMap<String, String>>,
    pub failed: Vec<ModerateItem>,
}

fn map_approval_status(status: &str) -> u32 {
    match status.to_lowercase().as_str() {
        "approve" | "approved" => 0,
        "reject" | "deny" | "denied" | "rejected" => 1,
        "pending" => 2,
        "draft" => 3,
        "scheduled" => 4,
        _ => 2,
    }
}

/// Moderate items from a SharePoint List.
///
/// Corresponds to `$SP().list.moderate`.
pub async fn moderate(
    list_id: &str,
    site_url: &str,
    client: &Client,
    items: Vec<ModerateItem>,
    options: Option<ModerateOptions>,
) -> Result<ModerateResult, ModerateError> {
    if list_id.is_empty() {
        return Err(ModerateError::MissingListId);
    }
    if site_url.is_empty() {
        return Err(ModerateError::MissingSiteUrl);
    }

    let options = options.unwrap_or_default();
    let packet_size = if options.packet_size > 0 { options.packet_size } else { 15 };
    let total_items = items.len();
    
    let mut passed = Vec::new();
    let mut failed = Vec::new();
    let mut current_count = 0;

    for chunk in items.chunks(packet_size) {
        let mut updates = String::from(r#"<Batch OnError="Continue" ListVersion="1" ViewName="">"#);
        
        for (i, item) in chunk.iter().enumerate() {
            if item.id == 0 {
                return Err(ModerateError::MissingItemId);
            }
            if item.approval_status.is_empty() {
                return Err(ModerateError::MissingApprovalStatus);
            }

            let status_val = map_approval_status(&item.approval_status);
            
            // Method ID is 1-based index within the chunk
            updates.push_str(&format!(r#"<Method ID="{}" Cmd="Moderate">"#, i + 1));
            updates.push_str(&format!(r#"<Field Name="ID">{}</Field>"#, item.id));
            updates.push_str(&format!(r#"<Field Name="_ModerationStatus">{}</Field>"#, status_val));
            
            for (key, val) in &item.other_fields {
                if key != "ID" && key != "ApprovalStatus" && key != "_ModerationStatus" {
                     updates.push_str(&format!(r#"<Field Name="{}">{}</Field>"#, key, val));
                }
            }
            updates.push_str("</Method>");
        }
        updates.push_str("</Batch>");

        let soap_body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <UpdateListItems xmlns="http://schemas.microsoft.com/sharepoint/soap/">
      <listName>{}</listName>
      <updates>{}</updates>
    </UpdateListItems>
  </soap:Body>
</soap:Envelope>"#,
            list_id, updates
        );

        let request_url = format!("{}/_vti_bin/lists.asmx", site_url.trim_end_matches('/'));
        
        let response = client.post(&request_url)
            .header("SOAPAction", "http://schemas.microsoft.com/sharepoint/soap/UpdateListItems")
            .header("Content-Type", "text/xml; charset=utf-8")
            .body(soap_body)
            .send()
            .await?;

        let response_text = response.text().await?;
        
        // Parse XML
        let mut reader = Reader::from_str(&response_text);
        reader.trim_text(true);
        let mut buf = Vec::new();
        
        let mut current_method_id: Option<usize> = None;
        let mut current_error_code: Option<String> = None;
        let mut current_error_text: Option<String> = None;
        let mut current_row_data: Option<HashMap<String, String>> = None;
        let mut in_result = false;
        
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    match e.name().as_ref() {
                        b"Result" => {
                            in_result = true;
                            current_method_id = None;
                            current_error_code = None;
                            current_error_text = None;
                            current_row_data = None;

                            for attr in e.attributes() {
                                let attr = attr?;
                                if attr.key.as_ref() == b"ID" {
                                    let val = attr.decode_and_unescape_value(&reader)?.to_string();
                                    // Format is usually "MethodID,Cmd" e.g. "1,Moderate"
                                    let parts: Vec<&str> = val.split(',').collect();
                                    if let Some(id_str) = parts.first() {
                                        if let Ok(id) = id_str.parse::<usize>() {
                                            current_method_id = Some(id);
                                        }
                                    }
                                }
                            }
                        },
                        b"ErrorCode" => {
                            if in_result {
                                // Read text content
                                let mut txt_buf = Vec::new();
                                if let Ok(Event::Text(e)) = reader.read_event_into(&mut txt_buf) {
                                    current_error_code = Some(e.unescape()?.to_string());
                                }
                            }
                        },
                        b"ErrorText" => {
                            if in_result {
                                let mut txt_buf = Vec::new();
                                if let Ok(Event::Text(e)) = reader.read_event_into(&mut txt_buf) {
                                    current_error_text = Some(e.unescape()?.to_string());
                                }
                            }
                        },
                        b"z:row" | b"row" => {
                            if in_result {
                                let mut row_data = HashMap::new();
                                for attr in e.attributes() {
                                    let attr = attr?;
                                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                    let value = attr.decode_and_unescape_value(&reader)?.to_string();
                                    let clean_key = key.strip_prefix("ows_").unwrap_or(&key).to_string();
                                    row_data.insert(clean_key, value);
                                }
                                current_row_data = Some(row_data);
                            }
                        }
                        _ => (),
                    }
                },
                Ok(Event::End(e)) => {
                    if e.name().as_ref() == b"Result" {
                        in_result = false;
                        if let Some(method_id) = current_method_id {
                            if method_id > 0 && method_id <= chunk.len() {
                                let mut item = chunk[method_id - 1].clone();
                                let is_success = current_error_code.as_deref() == Some("0x00000000");
                                
                                if is_success {
                                    if let Some(row) = current_row_data.take() {
                                        passed.push(row);
                                    } else {
                                        // Fallback if no row returned (unlikely for UpdateListItems but possible)
                                        // Create a hashmap from the input item
                                        let mut row = item.other_fields.clone();
                                        row.insert("ID".to_string(), item.id.to_string());
                                        row.insert("ApprovalStatus".to_string(), item.approval_status.clone());
                                        passed.push(row);
                                    }
                                } else {
                                    item.error_message = current_error_text.clone();
                                    failed.push(item);
                                }
                            }
                        }
                    }
                },
                Ok(Event::Eof) => break,
                Err(e) => return Err(ModerateError::XmlError(e)),
                _ => (),
            }
            buf.clear();
        }
        
        current_count += chunk.len();
        if let Some(progress) = &options.progress {
            progress(current_count, total_items);
        }
    }

    Ok(ModerateResult { passed, failed })
}