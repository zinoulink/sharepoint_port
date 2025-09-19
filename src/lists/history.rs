use reqwest::Client;
use quick_xml::events::{Event, BytesStart};
use quick_xml::Reader;
use std::collections::HashMap;

// A struct to hold the necessary SharePoint context.
pub struct SharePointClient {
    pub url: String,
    pub list_id: String,
    http_client: Client,
}

// A struct to represent a single version from the history.
#[derive(Debug)]
pub struct Version {
    pub modified: String,
    pub editor: String,
    pub content: String,
}

impl SharePointClient {
    // Constructor to create a new client instance.
    pub fn new(url: &str, list_id: &str) -> Self {
        SharePointClient {
            url: url.to_string(),
            list_id: list_id.to_string(),
            http_client: Client::new(),
        }
    }

    // The equivalent of the JavaScript history function.
    // It's an async function that returns a Result.
    pub async fn history(&self, item_id: &str, field_name: &str) -> Result<Vec<Version>, Box<dyn std::error::Error>> {
        // Validate inputs. In Rust, we use Result for error handling.
        if self.list_id.is_empty() {
            return Err("The list ID is required.".into());
        }
        if item_id.is_empty() || field_name.is_empty() {
            return Err("You must provide the item ID and field Name.".into());
        }

        // Construct the SOAP request body.
        let soap_body = format!(r#"
            <?xml version="1.0" encoding="utf-8"?>
            <soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
              <soap:Body>
                <GetVersionCollection xmlns="http://schemas.microsoft.com/sharepoint/soap/">
                  <strlistID>{}</strlistID>
                  <strlistItemID>{}</strlistItemID>
                  <strFieldName>{}</strFieldName>
                </GetVersionCollection>
              </soap:Body>
            </soap:Envelope>
        "#, self.list_id, item_id, field_name);

        // Make the asynchronous HTTP request.
        let url = format!("{}/_vti_bin/lists.asmx", self.url);
        let response = self.http_client
            .post(&url)
            .header("SOAPAction", "http://schemas.microsoft.com/sharepoint/soap/GetVersionCollection")
            .header("Content-Type", "text/xml; charset=utf-8")
            .body(soap_body)
            .send()
            .await?
            .text()
            .await?;

        // Parse the XML response.
        let mut versions = Vec::new();
        let mut reader = Reader::from_str(&response);
        let mut buf = Vec::new();
        let mut current_version_attributes: HashMap<String, String> = HashMap::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) if e.name().as_ref() == b"Version" => {
                    // Found a <Version> tag, parse its attributes.
                    current_version_attributes.clear();
                    for attribute in e.attributes() {
                        if let Ok(attr) = attribute {
                            let key = String::from_utf8(attr.key.into_inner().to_vec())?;
                            let value = String::from_utf8(attr.value.into_owned())?;
                            current_version_attributes.insert(key, value);
                        }
                    }

                    // Extract the desired attributes.
                    let content = current_version_attributes.get(field_name).cloned().unwrap_or_default();
                    let version = Version {
                        modified: current_version_attributes.get("Modified").cloned().unwrap_or_default(),
                        editor: current_version_attributes.get("Editor").cloned().unwrap_or_default(),
                        content,
                    };
                    versions.push(version);
                }
                Ok(Event::Eof) => break, // End of file, break the loop.
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // Ignore other events.
            }
        }

        Ok(versions)
    }
}