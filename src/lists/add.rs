use reqwest::Client;
use xml::reader::{EventReader, XmlEvent};
use uuid::Uuid;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Debug)]
struct ListAddOptions {
    packetsize: usize,
    progress: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
    break_on_failure: bool,
    escape_char: bool,
    root_folder: String,
}

impl Default for ListAddOptions {
    fn default() -> Self {
        ListAddOptions {
            packetsize: 30,
            progress: None,
            break_on_failure: false,
            escape_char: true,
            root_folder: String::new(),
        }
    }
}

async fn add(items: Vec<HashMap<String, String>>, options: ListAddOptions, list_id: &str, url: &str) -> Result<(Vec<HashMap<String, String>>, Vec<HashMap<String, String>>), Box<dyn std::error::Error>> {
    let client = Client::new();
    let mut passed = Vec::new();
    let mut failed = Vec::new();

    for chunk in items.chunks(options.packetsize) {
        let mut updates = String::new();
        for (i, item) in chunk.iter().enumerate() {
            updates.push_str(&format!(r#"<Method ID="{}" Cmd="New">"#, i + 1));
            updates.push_str(r#"<Field Name='ID'>New</Field>"#);
            for (key, value) in item {
                updates.push_str(&format!(r#"<Field Name='{}'>{}</Field>"#, key, value));
            }
            updates.push_str("</Method>");
        }

        let body = format!(r#"<listName>{}</listName><updates><Batch OnError="Continue" ListVersion="1" ViewName=""{}>{}</Batch></updates>"#,
            list_id,
            if !options.root_folder.is_empty() { format!(r#" RootFolder="{}""#, options.root_folder) } else { String::new() },
            updates
        );

        let response = client.post(&format!("{}/_vti_bin/lists.asmx", url))
            .header("SOAPAction", "http://schemas.microsoft.com/sharepoint/soap/UpdateListItems")
            .body(body)
            .send()
            .await?;

        let content = response.text().await?;
        let parser = EventReader::from_str(&content);
        for event in parser {
            match event {
                Ok(XmlEvent::StartElement { name, .. }) if name.local_name == "Result" => {
                    // Parse the result and update passed or failed vectors
                },
                _ => {}
            }
        }
    }

    Ok((passed, failed))
}

#[tokio::main]
async fn main() {
    let items = vec![
        vec![("Title".to_string(), "Ok".to_string())].into_iter().collect(),
    ];
    let options = ListAddOptions::default();
    match add(items, options, "My List", "http://my.sharepoi.nt/dir/").await {
        Ok((passed, failed)) => {
            println!("Passed: {:?}", passed);
            println!("Failed: {:?}", failed);
        },
        Err(e) => println!("Error: {}", e),
    }
}