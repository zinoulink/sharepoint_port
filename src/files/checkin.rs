use std::collections::HashMap;
use reqwest::Client;
use xml::builder::*;

#[derive(Debug)]
pub struct Error(String);

pub async fn checkin(setup: Option<HashMap<String, String>>) -> Result<(), Error> {
    let mut setup = setup.unwrap_or_default();

    // Validate destination
    if !setup.contains_key("destination") {
        return Err(Error("[SharepointSharp 'checkin'] the file destination path is required.".to_string()));
    }

    // Set URL if not provided
    if !setup.contains_key("url") {
        // Implement your logic to get the URL (replace with your actual implementation)
        let url = get_url().await?; // Replace with your URL retrieval function
        setup.insert("url".to_string(), url);
    }

    // Escape comments for XML
    setup.insert("comments".to_string(), escape_xml(&setup["comments"].to_owned())?);

    let checkin_type = match setup.get("type") {
        Some("MinorCheckIn") => 0,
        Some("OverwriteCheckIn") => 2,
        _ => 1, // Default to MajorCheckIn
    };

    let url = format!("{}/_vti_bin/Lists.asmx", setup["url"].to_owned());
    let soap_body = build_soap_body(
        "CheckInFile",
        vec![
            ("pageUrl", &setup["destination"]),
            ("comment", &setup["comments"]),
            ("CheckinType", &checkin_type.to_string()),
        ],
    );

    let client = Client::new();
    let response = client
        .post(&url)
        .header("SOAPAction", "http://schemas.microsoft.com/sharepoint/soap/CheckInFile")
        .body(soap_body.as_bytes())
        .send()
        .await?;

    if response.status().is_success() {
        let response_xml = response.text().await?;
        let doc = Document::parse_str(&response_xml)?;
        let result_node = doc.find("//CheckInFileResult").unwrap();

        if result_node.text() != "true" {
            return Err(Error(format!(
                "Check-in failed: {}",
                result_node.text().unwrap_or_default()
            )));
        }
    } else {
        return Err(Error(format!("Failed to check in: {}", response.status())));
    }

    Ok(())
}

// Implement these functions as needed for your specific environment:
async fn get_url() -> Result<String, Error> {
    // Your logic to retrieve the URL
    Err(Error("Not implemented".to_string()))
}

fn escape_xml(text: &str) -> Result<String, Error> {
    // Implement XML escaping logic
    Err(Error("Not implemented".to_string()))
}

fn build_soap_body(method: &str, elements: Vec<(&str, &str)>) -> String {
    let mut builder = Builder::new();
    builder = builder.element("soapenv:Envelope", {
        attr("xmlns:soapenv", "http://schemas.xmlsoap.org/soap/envelope/");
        attr("xmlns:ns0", "http://schemas.microsoft.com/sharepoint/soap/");
    });
    builder = builder.push(builder::element("soapenv:Header", Vec::new()));
    builder = builder.push(builder::element(
        "soapenv:Body",
        vec![builder::element(
            "ns0:".to_owned() + method,
            elements.into_iter().map(|&(name, value)| {
                builder::element(name, value)
            }).collect(),
        )],
    ));

    builder.as_str().to_string()
}