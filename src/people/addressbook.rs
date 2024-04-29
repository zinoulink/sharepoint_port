// Assuming you have a struct or enum to represent the result
struct AddressBookResult {
    AccountName: String,
    UserInfoID: String,
    DisplayName: String,
    Email: String,
    Department: String,
    Title: String,
    PrincipalType: String,
}

// Function to find a user based on a part of their name
async fn addressbook(username: &str, setup: Option<(&str, &str)>) -> Result<Vec<AddressBookResult>, Box<dyn std::error::Error>> {
    let mut username = username.to_string();
    let mut setup = setup.unwrap_or_default();

    match setup {
        (word, _) => {
            if !word.is_empty() {
                username = word.to_string();
            }
        }
    }

    // Your logic for making the request to SharePoint goes here
    // For example, using reqwest or any other HTTP client library

    // Simulating the result
    let result = vec![AddressBookResult {
        AccountName: "john.doe".to_string(),
        UserInfoID: "12345".to_string(),
        DisplayName: "John Doe".to_string(),
        Email: "john@example.com".to_string(),
        Department: "IT".to_string(),
        Title: "Software Engineer".to_string(),
        PrincipalType: "User".to_string(),
    }];

    Ok(result)
}

// Example usage
fn main() {
    match addressbook("john", Some(("limit", "25"))) {
        Ok(people) => {
            for person in people {
                println!("Display Name: {}", person.DisplayName);
                println!("Email: {}", person.Email);
                // Add other fields as needed
            }
        }
        Err(err) => eprintln!("Error: {}", err),
    }
}
