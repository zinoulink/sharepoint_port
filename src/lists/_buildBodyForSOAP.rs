/// Builds the body for a SOAP request.
///
/// # Arguments
/// * `method_name` - The name of the SOAP method.
/// * `body_content` - The content to be included in the SOAP body.
/// * `xmlns` - The XML namespace (defaults to "http://schemas.microsoft.com/sharepoint/soap/").
///
/// # Returns
/// A `String` containing the SOAP request body.
pub fn build_body_for_soap(method_name: &str, body_content: &str, xmlns: Option<&str>) -> String {
    let xmlns = xmlns.unwrap_or("http://schemas.microsoft.com/sharepoint/soap/");
    let xmlns = xmlns.replace("webpartpages/", "webpartpages");

    format!(
        r#"<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
            <soap:Body>
                <{method_name} xmlns="{xmlns}">
                    {body_content}
                </{method_name}>
            </soap:Body>
        </soap:Envelope>"#,
        method_name = method_name,
        xmlns = xmlns,
        body_content = body_content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_body_for_soap() {
        let method_name = "GetListItems";
        let body_content = "<listName>MyList</listName>";
        let xmlns = "http://schemas.microsoft.com/sharepoint/soap/";

        let expected = r#"<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
            <soap:Body>
                <GetListItems xmlns="http://schemas.microsoft.com/sharepoint/soap/">
                    <listName>MyList</listName>
                </GetListItems>
            </soap:Body>
        </soap:Envelope>"#;

        let result = build_body_for_soap(method_name, body_content, Some(xmlns));
        assert_eq!(result, expected);
    }
}