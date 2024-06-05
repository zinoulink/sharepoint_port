// Assuming you have the necessary imports and setup in your Rust project
// You'll need to adapt this code to your specific context

use reqwest::blocking::Client; // Example HTTP client library for making requests

async fn is_member(setup: &Setup) -> Result<bool, String> {
    // Error handling omitted for brevity
    let url = format!("{}/_vti_bin/usergroup.asmx", setup.url);
    let body = format!(
        r#"<userLoginName>{}</userLoginName>"#,
        setup.user
    );

    // Make an HTTP request to the SharePoint API
    let response = Client::new()
        .post(&url)
        .body(body)
        .send()
        .map_err(|err| format!("Error sending request: {}", err))?;

    // Parse the XML response and check if the user is a member of the group
    let data = parse_response(response)?;
    let groups = data.get("UserGroups").unwrap_or_default();
    let group_names: Vec<String> = groups.iter().map(|g| g.to_lowercase()).collect();

    if group_names.contains(&setup.group.to_lowercase()) {
        return Ok(true);
    }

    // If not found in user groups, check distribution lists
    let members = get_group_members(setup.group, &setup.url)?;
    let distrib = get_distribution_lists(setup.user, &setup.url)?;

    for member in members {
        if distrib.contains(&member.to_lowercase()) {
            return Ok(true);
        }
    }

    Ok(false)
}

// Define your data structures (Setup, UserInfo, etc.) as needed

fn main() {
    // Example usage
    let setup = Setup {
        user: "john.doe".to_string(),
        group: "developers".to_string(),
        url: "https://example.com".to_string(),
        cache: true,
    };

    match is_member(&setup) {
        Ok(is_member) => {
            if is_member {
                println!("User is a member of the group.");
            } else {
                println!("User is not a member of the group.");
            }
        }
        Err(err) => eprintln!("Error: {}", err),
    }
}
