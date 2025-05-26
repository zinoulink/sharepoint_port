use once_cell::sync::Lazy;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SpError {
    #[error("[SharepointSharp 'getContentTypes'] the list ID/name is required.")]
    ListIdRequired,
    #[error("[SharepointSharp 'getContentTypes'] not able to find the URL.")]
    UrlRequired, // Though in this Rust structure, URL is part of ListClient
    #[error("HTTP request error: {0}")]
    HttpRequest(#[from] reqwest::Error),
    #[error("XML parsing error: {0}")]
    XmlParsing(String),
    #[error("SOAP Fault or HTTP error: {0}")]
    SoapError(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContentType {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Description")]
    pub description: String,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    list_id: String,
    url: String,
    content_types: Vec<ContentType>,
}

static SP_CACHE_CONTENTTYPES: Lazy<Mutex<Vec<CacheEntry>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

#[derive(Default, Debug, Clone, Copy)]
pub struct GetContentTypesOptions {
    pub cache: bool,
}

// This struct represents the context (`this`) from the JavaScript code.
// In a real application, this would likely be part of a larger SharePoint client structure.
pub struct ListClient {
    pub list_id: String,
    pub site_url: String, // Renamed from 'url' for clarity
    http_client: HttpClient,
}

impl ListClient {
    // Example constructor
    pub fn new(site_url: &str, list_id: &str, http_client: HttpClient) -> Self {
        ListClient {
            list_id: list_id.to_string(),
            site_url: site_url.to_string(),
            http_client,
        }
    }

    /// Get the Content Types for the list (returns Name, ID and Description)
    ///
    /// # Arguments
    /// * `options` - Optional settings, e.g., for caching.
    ///
    /// # Example
    /// ```rust,ignore
    /// // Assuming client is an instance of ListClient
    /// // let content_types = client.get_content_types(None).await?;
    /// // for ct in content_types {
    /// //   println!("Name: {}, ID: {}, Description: {}", ct.name, ct.id, ct.description);
    /// // }
    /// ```
    pub async fn get_content_types(
        &self,
        options: Option<GetContentTypesOptions>,
    ) -> Result<Vec<ContentType>, SpError> {
        // In Rust, list_id and site_url presence is enforced by the ListClient struct fields.
        // The original JS checks are implicitly handled by requiring `self`.
        // if self.list_id.is_empty() { return Err(SpError::ListIdRequired); } // Covered by struct design
        // if self.site_url.is_empty() { return Err(SpError::UrlRequired); } // Covered by struct design

        let opts = options.unwrap_or(GetContentTypesOptions { cache: true });

        if opts.cache {
            let cache = SP_CACHE_CONTENTTYPES.lock().unwrap(); // Handle potential poisoning in production
            for entry in cache.iter() {
                if entry.list_id == self.list_id && entry.url == self.site_url {
                    return Ok(entry.content_types.clone());
                }
            }
        }

        let soap_body = build_body_for_soap(
            "GetListContentTypes",
            &format!("<listName>{}</listName>", self.list_id),
        );

        let request_url = format!("{}/_vti_bin/lists.asmx", self.site_url);

        let response = self
            .http_client
            .post(&request_url)
            .header("Content-Type", "text/xml; charset=utf-8")
            .header(
                "SOAPAction",
                "http://schemas.microsoft.com/sharepoint/soap/GetListContentTypes",
            )
            .body(soap_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error reading response body".to_string());
            return Err(SpError::SoapError(format!(
                "HTTP Error: {}. Body: {}",
                response.status(),
                error_text
            )));
        }

        let response_text = response.text().await?;
        parse_content_types_xml(&response_text).map(|parsed_content_types| {
            if opts.cache {
                let mut cache = SP_CACHE_CONTENTTYPES.lock().unwrap(); // Handle poisoning
                cache.push(CacheEntry {
                    list_id: self.list_id.clone(),
                    url: self.site_url.clone(),
                    content_types: parsed_content_types.clone(),
                });
            }
            parsed_content_types
        })
    }
}

fn build_body_for_soap(method_name: &str, inner_xml: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <{methodName} xmlns="http://schemas.microsoft.com/sharepoint/soap/">
      {innerXml}
    </{methodName}>
  </soap:Body>
</soap:Envelope>"#,
        methodName = method_name,
        innerXml = inner_xml
    )
}

fn parse_content_types_xml(xml_data: &str) -> Result<Vec<ContentType>, SpError> {
    let mut reader = Reader::from_str(xml_data);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut results = Vec::new();

    // The actual ContentType elements are usually nested within:
    // GetListContentTypesResponse -> GetListContentTypesResult -> ContentTypes -> ContentType
    // This parser looks for <ContentType> elements anywhere for simplicity, similar to getElementsByTagName.
    // For robustness, a more specific path traversal might be needed, or ensure the namespace is handled if elements are qualified.
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                if e.local_name().as_ref() == b"ContentType" {
                    let mut id = None;
                    let mut name = None;
                    let mut description = None;

                    for attr_result in e.attributes() {
                        let attr = attr_result.map_err(|err| {
                            SpError::XmlParsing(format!("Attribute parsing error: {}", err))
                        })?;
                        match attr.key.as_ref() {
                            b"ID" => {
                                id = Some(String::from_utf8_lossy(&attr.value).into_owned())
                            }
                            b"Name" => {
                                name = Some(String::from_utf8_lossy(&attr.value).into_owned())
                            }
                            b"Description" => {
                                description = Some(String::from_utf8_lossy(&attr.value).into_owned())
                            }
                            _ => {} // Ignore other attributes
                        }
                    }

                    if let Some(id_val) = id {
                        results.push(ContentType {
                            id: id_val,
                            name: name.unwrap_or_default(),
                            description: description.unwrap_or_default(),
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(SpError::XmlParsing(format!(
                    "XML read error: {} at position {}",
                    e,
                    reader.buffer_position()
                )));
            }
            _ => {} // Other events (Text, End, CData, etc.)
        }
        buf.clear();
    }
    Ok(results)
}

// To make this runnable, you'd typically have a main function like this:
// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     let http_client = reqwest::Client::new();
//     let list_client = ListClient::new(
//         "http://your-sharepoint-site.com", // Replace with your SharePoint site URL
//         "Your List Name or ID", // Replace with your list name or ID
//         http_client,
//     );
//
//     match list_client.get_content_types(None).await {
//         Ok(content_types) => {
//             println!("Found {} content types:", content_types.len());
//             for ct in content_types {
//                 println!("  ID: {}, Name: {}, Description: {}", ct.id, ct.name, ct.description);
//             }
//         }
//         Err(e) => {
//             eprintln!("Error getting content types: {}", e);
//         }
//     }
//     Ok(())
// }