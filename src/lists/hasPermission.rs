// main.rs
use std::collections::HashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct EffectiveBasePermissions {
    #[serde(rename = "High")]
    high: u32,
    #[serde(rename = "Low")]
    low: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct PermissionsData {
    #[serde(rename = "d")]
    effective_base_permissions: EffectiveBasePermissions,
}

struct SharepointClient {
    list_id: String,
    url: String,
    client: Client,
}

impl SharepointClient {
    fn new(list_id: String, url: String) -> Self {
        Self {
            list_id,
            url,
            client: Client::new(),
        }
    }

    async fn has_permission(&self, perms: Vec<&str>) -> Result<HashMap<&str, bool>, String> {
        let perm_match: HashMap<&str, u32> = [
            ("emptyMask", 0),
            ("viewListItems", 1),
            ("addListItems", 2),
            ("editListItems", 3),
            ("deleteListItems", 4),
            ("approveItems", 5),
            ("openItems", 6),
            ("viewVersions", 7),
            ("deleteVersions", 8),
            ("cancelCheckout", 9),
            ("managePersonalViews", 10),
            ("manageLists", 12),
            ("viewFormPages", 13),
            ("anonymousSearchAccessList", 14),
            ("open", 17),
            ("viewPages", 18),
            ("addAndCustomizePages", 19),
            ("applyThemeAndBorder", 20),
            ("applyStyleSheets", 21),
            ("viewUsageData", 22),
            ("createSSCSite", 23),
            ("manageSubwebs", 24),
            ("createGroups", 25),
            ("managePermissions", 26),
            ("browseDirectories", 27),
            ("browseUserInfo", 28),
            ("addDelPrivateWebParts", 29),
            ("updatePersonalWebParts", 30),
            ("manageWeb", 31),
            ("anonymousSearchAccessWebLists", 32),
            ("useClientIntegration", 37),
            ("useRemoteAPIs", 38),
            ("manageAlerts", 39),
            ("createAlerts", 40),
            ("editMyUserInfo", 41),
            ("enumeratePermissions", 63),
            ("fullMask", 65),
        ]
        .iter()
        .cloned()
        .collect();

        // 1. Input validation
        if self.list_id.is_empty() {
            return Err("[SharepointSharp 'hasPermission'] the list ID/Name is required.".to_string());
        }
        if self.url.is_empty() {
            return Err("[SharepointSharp 'hasPermission'] not able to find the URL!".to_string());
        }

        // Check if all requested permissions are valid
        for perm in &perms {
            if !perm_match.contains_key(perm) {
                return Err(format!("[SharepointSharp 'hasPermission'] the permission '{perm}' is not valid. Please, check the documentation."));
            }
        }

        // 2. Build the API URL and make the request
        let request_url = format!(
            "{}/_api/web/lists/getbytitle('{}')/EffectiveBasePermissions",
            self.url, self.list_id
        );

        let response = self.client
            .get(&request_url)
            .header("Accept", "application/json;odata=verbose") // SharePoint requires this header
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let data: PermissionsData = response
            .json()
            .await
            .map_err(|e| e.to_string())?;

        let server_perm = data.effective_base_permissions;
        let mut ret = HashMap::new();

        // 3. Bitwise permission check logic
        for perm in perms {
            let perm_bit = perm_match.get(perm).unwrap();

            if *perm_bit == 65 {
                let has_perm = (server_perm.high & 32767) == 32767 && server_perm.low == 65535;
                ret.insert(perm, has_perm);
                continue;
            }

            let a = perm_bit - 1;
            let mut b = 1;

            let has_perm = if a >= 0 && a < 32 {
                b <<= a;
                (server_perm.low & b) != 0
            } else if a >= 32 && a < 64 {
                b <<= a - 32;
                (server_perm.high & b) != 0
            } else {
                false
            };

            ret.insert(perm, has_perm);
        }

        Ok(ret)
    }
}