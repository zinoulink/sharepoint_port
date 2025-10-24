use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use thiserror::Error;

/// Represents the details of a SharePoint list.
pub type ListDetails = HashMap<String, String>;
/// Represents the properties of a single list field.
pub type FieldInfo = HashMap<String, JsonValue>;

/// Contains the detailed information about a list, including its properties and fields.
#[derive(Debug, Clone)]
pub struct ListInfo {
    pub list_details: ListDetails,
    pub fields: Vec<FieldInfo>,
}

/// Errors that can occur when fetching list information.
#[derive(Debug, Error)]
pub enum GetInfoError {
    #[error("[SharepointSharp 'info'] The list ID/Name is required.")]
    MissingListId,
    #[error("[SharepointSharp 'info'] The site URL is required.")]
    MissingSiteUrl,
    #[error("HTTP request to SharePoint failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("XML parsing failed: {0}")]
    XmlError(#[from] quick_xml::Error),
    #[error("XML attribute parsing failed: {0}")]
    XmlAttrError(#[from] quick_xml::events::attributes::AttrError),
    #[error("SharePoint API returned an error: Status {status} - Body: {body}")]
    SharePointApiError {
        status: StatusCode,
        body: String,
    },
    #[error("Failed to parse SharePoint SOAP response: {0}")]
    ResponseParseError(String),
}

/// A struct to hold the necessary SharePoint context.
/// In a real app, this would be part of a larger client structure.
pub struct ListContext<'a> {
    pub list_id: &'a str,
    pub url: &'a reqwest::Url,
}

/// Get the columns' information/metadata, and the list's details.
/// Corresponds to the JavaScript function `$SP().list.info`.
///
/// # Arguments
/// * `ctx` - The context containing the list ID and site URL.
/// * `http_client` - An authenticated `reqwest::Client`.
///
/// # Returns
/// A `Result` containing the `ListInfo` on success, or a `GetInfoError`.
pub async fn get_list_info(
    ctx: ListContext<'_>,
    http_client: &Client,
) -> Result<ListInfo, GetInfoError> {
    if ctx.list_id.is_empty() {
        return Err(GetInfoError::MissingListId);
    }

    // Build SOAP request body
    let soap_body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <GetList xmlns="http://schemas.microsoft.com/sharepoint/soap/">
      <listName>{}</listName>
    </GetList>
  </soap:Body>
</soap:Envelope>"#,
        ctx.list_id
    );

    let request_url = ctx.url.join("_vti_bin/lists.asmx")?;

    let response = http_client
        .post(request_url)
        .header(
            "SOAPAction",
            "http://schemas.microsoft.com/sharepoint/soap/GetList",
        )
        .header("Content-Type", "text/xml; charset=utf-8")
        .body(soap_body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error body".to_string());
        return Err(GetInfoError::SharePointApiError { status, body });
    }

    let response_text = response.text().await?;
    parse_get_list_response(&response_text)
}

/// Parses the XML response from the `GetList` SOAP call.
fn parse_get_list_response(xml_data: &str) -> Result<ListInfo, GetInfoError> {
    let mut reader = Reader::from_str(xml_data);
    reader.trim_text(true);
    let mut buf = Vec::new();

    let mut list_details = ListDetails::new();
    let mut fields = Vec::<FieldInfo>::new();

    // Find the <List> element first to get its attributes
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) if e.name().as_ref() == b"List" => {
                for attr in e.attributes() {
                    let attr = attr?;
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let value = attr.decode_and_unescape_value(&reader)?.to_string();
                    list_details.insert(key, value);
                }
                // Found the list, now we can look for fields inside it
                break;
            }
            Event::Eof => {
                return Err(GetInfoError::ResponseParseError(
                    "Could not find <List> element in the SOAP response.".to_string(),
                ));
            }
            _ => (),
        }
        buf.clear();
    }

    // Now parse the <Field> elements
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) if e.name().as_ref() == b"Field" => {
                let mut field_info = parse_field_element(&mut reader, &e)?;
                // The JS code only adds fields that have an "ID" attribute.
                if field_info.contains_key("ID") {
                    fields.push(field_info);
                }
            }
            Event::End(e) if e.name().as_ref() == b"Fields" => {
                // We've reached the end of the fields section
                break;
            }
            Event::Eof => break,
            _ => (),
        }
        buf.clear();
    }

    Ok(ListInfo {
        list_details,
        fields,
    })
}

/// Parses a single `<Field>` element and its children.
fn parse_field_element(
    reader: &mut Reader<&[u8]>,
    start_element: &BytesStart,
) -> Result<FieldInfo, GetInfoError> {
    let mut field_info = FieldInfo::new();
    let mut field_type = String::new();

    // Parse attributes of the <Field> tag
    for attr in start_element.attributes() {
        let attr = attr?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let value = attr.decode_and_unescape_value(&reader)?.to_string();
        if key == "Type" {
            field_type = value.clone();
        }
        field_info.insert(key, JsonValue::String(value));
    }

    // Parse child elements like <CHOICES>, <Default>, etc.
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => match e.name().as_ref() {
                b"CHOICE" | b"Default" => {
                    if let Ok(Event::Text(text)) = reader.read_event_into(&mut Vec::new()) {
                        let value = text.unescape()?.to_string();
                        let key = if e.name().as_ref() == b"CHOICE" {
                            "Choices"
                        } else {
                            "DefaultValue"
                        };
                        field_info
                            .entry(key.to_string())
                            .or_insert_with(|| json!([]))
                            .as_array_mut()
                            .unwrap()
                            .push(json!(value));
                    }
                }
                // Handle other special child elements if necessary
                _ => (),
            },
            Event::End(e) if e.name().as_ref() == b"Field" => break,
            Event::Eof => break,
            _ => (),
        }
        buf.clear();
    }

    // Post-process special types like in the JS code
    match field_type.as_str() {
        "Lookup" | "LookupMulti" => {
            let list = field_info
                .get("List")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let field = field_info
                .get("ShowField")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            field_info.insert("Choices".to_string(), json!({ "list": list, "field": field }));
        }
        _ => {}
    }

    // Simplify single-value arrays for DefaultValue
    if let Some(JsonValue::Array(arr)) = field_info.get_mut("DefaultValue") {
        if arr.len() == 1 {
            let first_val = arr.remove(0);
            field_info.insert("DefaultValue".to_string(), first_val);
        }
    } else {
        field_info.insert("DefaultValue".to_string(), JsonValue::Null);
    }

    Ok(field_info)
}