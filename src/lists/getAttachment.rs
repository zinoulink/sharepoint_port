use crate::error::{Result, SpSharpError};
use crate::utils::build_soap_body; // Placeholder for SOAP envelope builder

use log::{debug, info, warn};
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest; // Make sure reqwest client is available (e.g., on SharePointList struct)
use url::Url; // Make sure Url is available (e.g., on SharePointList struct)


// Assuming SharePointList struct exists like this:
/*
use reqwest::Client;
use url::Url;
pub struct SharePointList {
    pub list_id: String,
    pub base_url: Url,
    pub client: Client,
}
*/

impl SharePointList {
    /// Get the attachment URL(s) for a specific list item.
    /// Corresponds to the JavaScript $SP().list.getAttachment function.
    ///
    /// # Arguments
    ///
    /// * `item_id` - The ID of the list item. Can be a number or string.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<String>` of attachment URLs on success,
    /// or an `SpSharpError` on failure.
    ///
    pub async fn get_attachment(&self, item_id: impl Into<String>) -> Result<Vec<String>> {
        // list_id and base_url availability are inherent to `self` existing.
        // Assume constructor ensures they are valid.

        let item_id_str = item_id.into();
        info!(
            "Requesting attachments for item '{}' in list '{}'",
            item_id_str, self.list_id
        );

        // 1. Construct SOAP Body
        let body_content = format!(
            "<listName>{}</listName><listItemID>{}</listItemID>",
            self.list_id, item_id_str
        );
        let soap_body = build_soap_body("GetAttachmentCollection", &body_content);
        debug!("SOAP Body for GetAttachmentCollection: {}", soap_body);

        // 2. Construct Request URL
        // Ensure the path ends correctly before joining.
        let request_url = self.base_url.join("_vti_bin/lists.asmx")?;
        debug!("Request URL: {}", request_url);


        // 3. Make HTTP Request
        let response = self.client
            .post(request_url)
            .header("Content-Type", "text/xml; charset=utf-8")
            .header(
                "SOAPAction",
                "http://schemas.microsoft.com/sharepoint/soap/GetAttachmentCollection",
            )
            .body(soap_body)
            .send()
            .await;

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                warn!("HTTP request failed for GetAttachmentCollection: {}", e);
                return Err(SpSharpError::HttpRequest(e));
            }
        };

        // 4. Check Response Status
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            warn!(
                "GetAttachmentCollection failed: Status={}, Body={}",
                status, error_text
            );
            // Consider parsing SOAP Fault here for a more specific error
            return Err(SpSharpError::SharePointError {
                code: status.to_string(),
                message: error_text,
            });
        }

        // 5. Parse XML Response
        let response_text = response.text().await?;
        debug!("SOAP Response: {}", response_text);

        let mut reader = Reader::from_str(&response_text);
        reader.trim_text(true);
        let mut buf = Vec::new();
        let mut attachments = Vec::new();
        let mut in_attachment_tag = false; // Flag to track if we are inside an <Attachment> tag

        loop {
            match reader.read_event_mut(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    // Check if the tag name is "Attachment"
                    if e.name().as_ref() == b"Attachment" {
                        in_attachment_tag = true;
                    }
                }
                Ok(Event::Text(e)) => {
                    // If we are inside an <Attachment> tag, this text is the URL
                    if in_attachment_tag {
                        let url_text = e.unescape_and_decode(&reader)?;
                         if !url_text.is_empty() {
                             attachments.push(url_text);
                         }
                        // Reset the flag after reading the text content
                        in_attachment_tag = false;
                    }
                }
                Ok(Event::End(ref e)) => {
                    // Reset flag if we encounter the closing </Attachment> tag
                    // (Handles cases like <Attachment/> potentially, though unlikely)
                    if e.name().as_ref() == b"Attachment" {
                        in_attachment_tag = false;
                    }
                }
                Ok(Event::Eof) => break, // End of document
                Err(e) => {
                    warn!("XML parsing error: {}", e);
                    return Err(SpSharpError::XmlParse(e));
                }
                _ => {} // Ignore other events like PIs, Comments, StartDocument, etc.
            }
            // important detail: clear the buffer between events
            buf.clear();
        }

        info!(
            "Found {} attachments for item '{}' in list '{}'",
            attachments.len(),
            item_id_str,
            self.list_id
        );
        Ok(attachments)
    }
}

// Add this to your main lib.rs or relevant module file
// pub mod get_attachment;