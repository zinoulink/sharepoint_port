fn group_members(groupname: &str, setup: &mut Setup) -> Result<Vec<UserInfo>, Box<dyn Error>> {
    if groupname.is_empty() {
        return Err("The groupname is required.".into());
    }

    // Default values
    setup.cache = setup.cache.unwrap_or(true);
    setup.url = setup.url.unwrap_or(get_url()?);

    let groupname_lowercase = groupname.to_lowercase();
    let url_lowercase = setup.url.to_lowercase();

    // Check the cache
    if setup.cache {
        for c in &mut global::_SP_CACHE_GROUPMEMBERS {
            if c.group == groupname_lowercase && c.url == url_lowercase {
                return Ok(c.data.clone());
            }
        }
    }

    // Send the request (simulated SOAP request)
    let data = fetch_user_data_from_sharepoint(&setup.url, &groupname)?;

    // Parse the response and extract user information
    let mut a_result = Vec::new();
    for user in data.iter() {
        a_result.push(UserInfo {
            id: user.get_attribute("ID")?,
            name: user.get_attribute("Name")?,
            login_name: user.get_attribute("LoginName")?,
            email: user.get_attribute("Email")?,
        });
    }

    // Cache the result
    let mut found = false;
    for c in &mut global::_SP_CACHE_GROUPMEMBERS {
        if c.group == groupname_lowercase && c.url == url_lowercase {
            c.data = a_result.clone();
            found = true;
            break;
        }
    }
    if !found {
        global::_SP_CACHE_GROUPMEMBERS.push(CacheEntry {
            group: groupname_lowercase,
            url: url_lowercase,
            data: a_result.clone(),
        });
    }

    Ok(a_result)
}

// Example usage
fn main() {
    let mut setup = Setup::default(); // Set your actual setup values
    let groupname = "MySharePointGroup"; // Replace with the actual group name
    match group_members(groupname, &mut setup) {
        Ok(members) => {
            for member in members {
                println!("User ID: {}, Name: {}, Email: {}", member.id, member.name, member.email);
            }
        }
        Err(err) => eprintln!("Error: {}", err),
    }
}
