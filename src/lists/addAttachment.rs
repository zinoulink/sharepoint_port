use reqwest::Client;
use base64::encode;
use xml::reader::{EventReader, XmlEvent};
use std::io::Cursor;
use std::error::Error;

#[derive(Debug)]
struct Setup {
    id: i32,
    filename: String,
    attachment: Vec<u8>,
}

async fn add_attachment(setup: Setup, list_id: &str, url: &str) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    // Sanitize filename
    let filename = sanitize_filename(&setup.filename);

    // Build SOAP request body
    let soap_body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
        <soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
            <soap:Body>
                <AddAttachment xmlns="http://schemas.microsoft.com/sharepoint/soap/">
                    <listName>{}</listName>
                    <listItemID>{}</listItemID>
                    <fileName>{}</fileName>
                    <attachment>{}</attachment>
                </AddAttachment>
            </soap:Body>
        </soap:Envelope>"#,
        list_id, setup.id, filename, encode(&setup.attachment)
    );

    // Make the SOAP request
    let response = client
        .post(&format!("{}/_vti_bin/Lists.asmx", url))
        .header("SOAPAction", "http://schemas.microsoft.com/sharepoint/soap/AddAttachment")
        .header("Content-Type", "text/xml; charset=utf-8")
        .body(soap_body)
        .send()
        .await?;

    let response_text = response.text().await?;

    // Parse the SOAP response
    let file_url = parse_soap_response(&response_text)?;

    // Handle versioning (pseudo-code, as getVersions and restoreVersion are not provided)
    // let versions = get_versions(setup.id).await?;
    // if let Some(last_version) = versions.last() {
    //     restore_version(setup.id, last_version.version_id).await?;
    // }

    Ok(file_url)
}

fn sanitize_filename(filename: &str) -> String {
    let mut sanitized = filename.replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "")
        .replace("..", ".")
        .trim_matches(|c: char| c == '.' || c == ' ')
        .to_string();

    if sanitized.len() >= 128 {
        sanitized = format!("{}__{}", &sanitized[..115], &sanitized[sanitized.len() - 8..]);
    }

    sanitized
}

fn parse_soap_response(xml: &str) -> Result<String, Box<dyn Error>> {
    let parser = EventReader::new(Cursor::new(xml));
    let mut in_add_attachment_result = false;

    for event in parser {
        match event? {
            XmlEvent::StartElement { name, .. } => {
                if name.local_name == "AddAttachmentResult" {
                    in_add_attachment_result = true;
                }
            }
            XmlEvent::Characters(text) => {
                if in_add_attachment_result {
                    return Ok(text);
                }
            }
            XmlEvent::EndElement { name } => {
                if name.local_name == "AddAttachmentResult" {
                    in_add_attachment_result = false;
                }
            }
            _ => {}
        }
    }

    Err("AddAttachmentResult not found in SOAP response".into())
}

#[tokio::main]
async fn main() {
    let setup = Setup {
        id: 1,
        filename: "helloworld.txt".to_string(),
        attachment: vec![0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x20, 0x57, 0x6F, 0x72, 0x6C, 0x64], // "Hello World" in ASCII
    };

    match add_attachment(setup, "My List", "https://your-sharepoint-site.com").await {
        Ok(file_url) => println!("Attachment added successfully: {}", file_url),
        Err(e) => eprintln!("Error adding attachment: {}", e),
    }
}