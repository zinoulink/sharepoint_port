use crate::lists::{get_content_types, build_body_for_soap};
use crate::utils::ajax;
use crate::cache::{SPCacheContentType, GLOBAL_SP_CACHE_CONTENTTYPE};
use crate::types::{FieldInfo, ContentTypeInfo};
use anyhow::{Result, anyhow};
use std::sync::Mutex;

pub async fn get_content_type_info(
    list_id: &str,
    url: &str,
    content_type: &str,
    options: Option<GetContentTypeInfoOptions>,
) -> Result<Vec<FieldInfo>> {
    let options = options.unwrap_or_default();

    // Check cache
    if options.cache {
        let cache = GLOBAL_SP_CACHE_CONTENTTYPE.lock().unwrap();
        if let Some(entry) = cache.iter().find(|entry| {
            entry.list == list_id && entry.url == url && entry.content_type == content_type
        }) {
            return Ok(entry.info.clone());
        }
    }

    // If not an ID, resolve name to ID
    if !content_type.starts_with("0x") {
        let types = get_content_types(list_id, url, options.clone()).await?;
        if let Some(ct) = types.iter().find(|ct| ct.name == content_type) {
            return get_content_type_info(list_id, url, &ct.id, Some(options)).await;
        }
        return Err(anyhow!(
            "Not able to find the Content Type called '{}' at {}",
            content_type,
            url
        ));
    }

    // SOAP request
    let soap_body = build_body_for_soap(
        "GetListContentType",
        &format!(
            "<listName>{}</listName><contentTypeId>{}</contentTypeId>",
            list_id, content_type
        ),
    );
    let data = ajax(
        url,
        "/_vti_bin/lists.asmx",
        &soap_body,
        Some("http://schemas.microsoft.com/sharepoint/soap/GetListContentType"),
    )
    .await?;

    // Parse XML response
    let fields = parse_fields_from_xml(&data)?;

    // Cache result
    {
        let mut cache = GLOBAL_SP_CACHE_CONTENTTYPE.lock().unwrap();
        cache.push(SPCacheContentType {
            list: list_id.to_string(),
            url: url.to_string(),
            content_type: content_type.to_string(),
            info: fields.clone(),
        });
    }

    Ok(fields)
}

// Define your options, cache, and XML parsing as needed
#[derive(Clone, Default)]
pub struct GetContentTypeInfoOptions {
    pub cache: bool,
}

// Implement parse_fields_from_xml to extract field info from the XML response
fn parse_fields_from_xml(xml: &str) -> Result<Vec<FieldInfo>> {
    // Use serde_xml_rs or quick-xml to parse the XML and extract fields
    // This is a placeholder for your actual implementation
    Ok(vec![]) // TODO: implement XML parsing
}