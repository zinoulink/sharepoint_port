use crate::utils::get_url; // Assuming get_url is in utils
use anyhow::{Ok, Result};
use url::Url;

/// Represents a client for interacting with SharePoint.
/// This struct holds the context, such as the site URL and the target list ID.
#[derive(Debug, Clone, Default)]
pub struct SharePointClient {
    pub list_id: Option<String>,
    pub site_url: Option<Url>,
    // You would add your reqwest::Client here for making HTTP requests
    // http_client: reqwest::Client,
}

impl SharePointClient {
    /// Creates a new, empty SharePoint client.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the client with a list name or ID and an optional site URL.
    ///
    /// This corresponds to the JavaScript function `$SP().list(list, url)`.
    ///
    /// # Arguments
    ///
    /// * `list_id` - The ID or name of the SharePoint list.
    /// * `url` - An optional site URL. If not provided, it will be discovered automatically.
    ///
    /// # Returns
    ///
    /// A mutable reference to the client for method chaining.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use your_crate::client::SharePointClient;
    ///
    /// async {
    ///     let mut client = SharePointClient::new();
    ///     client.list("My List", Some("http://my.sharepoi.nt/other.directory/")).await.unwrap();
    ///     // client is now configured to use "My List"
    ///
    ///     let mut client2 = SharePointClient::new();
    ///     client2.list("Another List", None).await.unwrap();
    ///     // client2's URL will be auto-discovered
    /// };
    /// ```
    pub async fn list(&mut self, list_id: &str, url: Option<&str>) -> Result<&mut Self> {
        // The JS version replaces '&' with '&amp;', which is good practice for XML/HTML contexts.
        self.list_id = Some(list_id.replace('&', "&amp;"));

        if let Some(base_url) = url {
            // Remove trailing slash if present and parse into a Url
            let clean_url = base_url.strip_suffix('/').unwrap_or(base_url);
            self.site_url = Some(Url::parse(clean_url)?);
        } else {
            // If no URL is provided, discover it.
            self.site_url = Some(get_url::discover_url().await?);
        }
        Ok(self)
    }
}