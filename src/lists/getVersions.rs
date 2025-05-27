use reqwest::{Client, StatusCode, header::ACCEPT};
use serde::Deserialize;
use serde_json::Value as JsonValue; // Using JsonValue for flexibility in version data
use thiserror::Error;

/**
  Represents a SharePoint list client.
*/
#[derive(Debug)]
pub struct SharePointList {
    list_id: String,
    site_url: String, // Base URL of the SharePoint site, e.g., "https://tenant.sharepoint.com/sites/MySite"
    client: Client,   // Pre-configured reqwest client (e.g., with authentication)
}

/// Errors that can occur when fetching list item versions.
#[derive(Debug, Error)]
pub enum GetVersionsError {
    #[error("[SharepointSharp 'getVersions'] The list ID/Name is required.")]
    MissingListId,
    #[error("[SharepointSharp 'getVersions'] Not able to find the URL!")]
    MissingSiteUrl,
    #[error("[SharepointSharp 'getVersions'] The item ID is required (cannot be zero).")]
    InvalidItemId,
    #[error("HTTP request to SharePoint failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("Failed to parse JSON response from SharePoint: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("SharePoint API returned an error: Status {status} - Body: {body}")]
    SharePointApiError {
        status: StatusCode,
        body: String,
    },
    #[error("Invalid response structure from SharePoint: missing 'd' and 'value' fields for results.")]
    InvalidResponseStructure,
}

// Structs to handle SharePoint OData JSON response structure
// This handles responses like { "d": { "results": [...] } } or { "value": [...] }
#[derive(Deserialize, Debug)]
struct ODataResponse<T> {
    d: Option<ODataResults<T>>,
    value: Option<Vec<T>>,
}

#[derive(Deserialize, Debug)]
struct ODataResults<T> {
    results: Vec<T>,
}

impl SharePointList {
    /// Creates a new SharePointList client.
    ///
    /// # Arguments
    /// * `list_id` - The ID or title of the SharePoint list.
    /// * `site_url` - The base URL of the SharePoint site.
    /// * `client` - A `reqwest::Client` pre-configured with any necessary authentication.
    pub fn new(list_id: String, site_url: String, client: Client) -> Self {
        Self { list_id, site_url, client }
    }

    /**
      When versioning is activated on a list, you can use this function to get the
      different versions of a list item.

      Corresponds to the Javascript function `$SP().list.getVersions`.

      @param item_id The item ID (must be a non-zero positive integer).
      @return A `Result` containing a `Vec` of `JsonValue` objects, where each `JsonValue`
              represents a version of the item, or a `GetVersionsError`.

      @example
      ```rust,ignore
      // Assuming `client` is a pre-configured reqwest::Client
      // and `SharePointList`, `GetVersionsError` are in scope.
      async {
          let sp_list = SharePointList::new(
              "My List".to_string(),
              "https://your-sharepoint-site.com".to_string(),
              client,
          );
          let item_id = 1234;

          match sp_list.get_versions(item_id).await {
              Ok(versions) => {
                  for version in versions {
                      println!("{:?}", version);
                  }
              }
              Err(e) => {
                  eprintln!("Error fetching versions: {}", e);
              }
          }
      };
      ```
    */
    pub async fn get_versions(&self, item_id: u32) -> Result<Vec<JsonValue>, GetVersionsError> {
        if self.list_id.is_empty() {
            return Err(GetVersionsError::MissingListId);
        }
        if self.site_url.is_empty() {
            return Err(GetVersionsError::MissingSiteUrl);
        }
        if item_id == 0 {
            // SharePoint item IDs are positive integers.
            return Err(GetVersionsError::InvalidItemId);
        }

        // Construct the API URL. Note: list_id might need URL encoding if it contains special characters,
        // but SharePoint's getbytitle often expects the direct title.
        let api_url = format!(
            "{}/_api/web/lists/getbytitle('{}')/Items({})/Versions",
            self.site_url.trim_end_matches('/'), // Ensure no double slashes
            self.list_id, // Consider potential need for single quote escaping if list_id can contain them
            item_id
        );

        let response = self.client.get(&api_url)
            // Requesting verbose OData to potentially get the { "d": { "results": ... } } structure,
            // but the parsing logic also handles the { "value": ... } structure.
            .header(ACCEPT, "application/json;odata=verbose")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            return Err(GetVersionsError::SharePointApiError { status, body });
        }

        let odata_response = response.json::<ODataResponse<JsonValue>>().await?;

        odata_response.d.map(|d| d.results)
            .or(odata_response.value)
            .ok_or(GetVersionsError::InvalidResponseStructure)
    }
}
*/
export default async function getVersions(itemID) {
  if (!this.listID) throw "[SharepointSharp 'getVersions'] the list ID/Name is required.";
  if (!this.url) throw "[SharepointSharp 'getVersions'] not able to find the URL!"; // we cannot determine the url
  if (!itemID) throw "[SharepointSharp 'getVersions'] the item ID is required.";

  return ajax.call(this, {
    url:this.url + "/_api/lists/getbytitle('"+this.listID+"')/Items("+itemID+")/Versions"
  })
  .then(res => {
    return ((res.d ? res.d.results : res.value)||[])
  })
}