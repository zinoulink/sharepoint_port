use reqwest::Client;
use xml::reader::{Parser, EventReader};
use xml::ElementReader;

#[derive(Debug)]
pub struct UserProfile {
    pub name: String,
    pub value: String,
}

pub async fn people(username: Option<String>, setup: Option<HashMap<String, String>>) -> Result<Vec<UserProfile>, reqwest::Error> {
    let username = username.unwrap_or_default();
    let mut setup = setup.unwrap_or_default();

    if !setup.contains_key("url") {
        let url = get_url().await?;
        setup.insert("url".to_string(), url);
    }

    let client = Client::new();
    let url = format!("{}/_vti_bin/UserProfileService.asmx", setup["url"].clone());

    let soap_body = build_body_for_soap("GetUserProfileByName", &format!("<AccountName>{}</AccountName>", username), "http://microsoft.com/webservices/SharePointPortalServer/UserProfileService");

    let response = client.post(&url)
        .header("SOAPAction", "http://microsoft.com/webservices/SharePointPortalServer/UserProfileService/GetUserProfileByName")
        .body(soap_body)
        .send()
        .await?;

    let mut result = Vec::new();
    let mut parser = Parser::new(response.text().await?);
    let mut reader = EventReader::from(parser);

    let mut current_tag: Option<String> = None;
    let mut current_name: Option<String> = None;
    let mut current_value: Option<String> = None;

    while let Some(e) = reader.next() {
        match e {
            Ok(reader::Event::StartElement { name, attributes }) => {
                current_tag = Some(name.local_name().to_string());
                if current_tag == Some("Name".to_string()) {
                    current_name = None;
                } else if current_tag == Some("Value".to_string()) {
                    current_value = None;
                }
            },
            Ok(reader::Event::Characters(text)) => {
                if current_tag == Some("Name".to_string()) {
                    current_name = Some(text.to_string());
                } else if current_tag == Some("Value".to_string()) {
                    current_value = Some(text.to_string());
                }
            },
            Ok(reader::Event::EndElement { name }) => {
                if name.local_name().to_string() == "PropertyData" {
                    if let (Some(name), Some(value)) = (current_name, current_value) {
                        result.push(UserProfile { name, value });
                    }
                    current_name = None;
                    current_value = None;
                }
                current_tag = None;
            },
            Err(e) => return Err(reqwest::Error::new(e.to_string())),
            _ => {}
        }
    }

    Ok(result)
}

fn build_body_for_soap(method: &str, body: &str, namespace: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <soap:Body>
    <{} xmlns="{}">
      {}
    </{}>
  </soap:Body>
</soap:Envelope>"#,
        method, namespace, body, method
    )
}

async fn get_url() -> Result<String, reqwest::Error> {
    // Implement your logic to get the URL here (replace with actual implementation)
    todo!("Implement get_url function to retrieve the URL")
}
