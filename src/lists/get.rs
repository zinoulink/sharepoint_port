use crate::error::{Result, SpSharpError};
use crate::utils::{
    build_soap_body, clean_string, get_lookup_id, parse_on_clause, parse_where_to_caml,
    to_sp_date_string, JoinFieldPair, // Import other needed utils
};
use crate::view::{self, ListContext as ViewContext, ViewDetails};
use crate::info::{self, ListContext as InfoContext, ListInfo};

use async_recursion::async_recursion;
use chrono::{DateTime, Duration, Utc};
use futures::future::try_join_all;
use itertools::Itertools;
use log::{debug, info, warn};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::mem;
use url::Url;

// Represents a single row/item from a SharePoint list
// Using HashMap for flexibility with dynamic fields and joins
pub type ListItem = HashMap<String, Option<String>>;

// Represents the Source list when merging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    list: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderOptions {
    #[serde(default)]
    path: String, // Relative path
    #[serde(default = "default_folder_show")]
    show: FolderShow,
    root_folder: Option<String>, // Full URL path to library root
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FolderShow {
    FilesOnlyRecursive,
    FilesAndFoldersRecursive,
    FilesOnlyInFolder,
    FilesAndFoldersInFolder, // Default
}

fn default_folder_show() -> FolderShow {
    FolderShow::FilesAndFoldersInFolder
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarOptions {
    #[serde(default = "default_split_recurrence")]
    split_recurrence: bool,
    #[serde(default = "Utc::now")]
    reference_date: DateTime<Utc>, // Store as DateTime, convert when building query
    #[serde(default = "default_calendar_range")]
    range: CalendarRange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CalendarRange {
    Month, // Default
    Week,
    Day, // Add if needed
}

fn default_split_recurrence() -> bool {
    true
}
fn default_calendar_range() -> CalendarRange {
    CalendarRange::Month
}

// Forward declaration for recursive type JoinOptions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetListItemsOptions {
    #[serde(default)]
    pub fields: Vec<String>,
    pub view: Option<String>,
    #[serde(default = "default_true")]
    pub view_cache: bool,
    #[serde(default)]
    pub json: bool, // Affects final return format (HashMap vs dedicated struct?)
    #[serde(default)]
    pub where_clause: WhereClause, // Use enum for String | Vec<String>
    #[serde(default)]
    pub where_caml: bool,
    #[serde(default = "default_true")]
    pub where_escape_char: bool,
    // pub where_fct: Option<Box<dyn Fn(String) -> String + Send + Sync>>, // Complex to handle well
    pub progress: Option<Box<dyn Fn(usize, Option<usize>) + Send + Sync>>, // (loaded, total) or (current_req, total_reqs)
    pub orderby: Option<String>,
    pub groupby: Option<String>,
    #[serde(default)]
    pub rowlimit: usize,
    #[serde(default)]
    pub paging: bool,
    #[serde(default = "default_page_limit")]
    pub page: usize, // Max number of pages/requests for paging
    #[serde(rename = "listItemCollectionPositionNext", default)]
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub use_index_for_orderby: bool,
    #[serde(default)]
    pub expand_user_field: bool,
    #[serde(default)]
    pub date_in_utc: bool,
    #[serde(default)]
    pub show_list_in_attribute: bool, // Add ListName prefix to attributes?
    pub alias: Option<String>, // Alias for the current list, used in joins/show_list_in_attribute
    pub merge: Option<Vec<MergeTarget>>, // Merge results from other lists
    pub folder_options: Option<FolderOptions>,
    pub query_options: Option<String>, // Raw XML override
    pub join: Option<Box<JoinOptions>>, // Use Box for recursion
    pub outerjoin: Option<Box<JoinOptions>>, // Mutually exclusive with join? JS allows nested
    #[serde(default)]
    pub calendar: bool,
    pub calendar_options: Option<CalendarOptions>,

    // --- Internal/Recursive State ---
    #[serde(skip)]
    results: Vec<ListItem>, // Accumulator for paging/multi-where
    #[serde(skip)]
    original_where: Option<WhereClause>, // Keep original if processing Vec
    #[serde(skip)]
    next_where: Vec<String>, // Remaining where clauses for multi-where
    #[serde(skip)]
    join_data: Option<JoinData>, // Data passed from parent list during join
    #[serde(skip)]
    merge_data: Vec<(ListItem, SourceInfo)>, // Accumulator for merging
    #[serde(skip)]
    is_join_child: bool, // Flag to know if this call is part of a join
     #[serde(skip)]
    calendar_via_view: bool, // If calendar was activated by a view setting
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)] // Allows parsing "where": "..." or "where": ["..."]
pub enum WhereClause {
    Single(String),
    Multiple(Vec<String>),
}

impl Default for WhereClause {
    fn default() -> Self { WhereClause::Single(String::new()) }
}

impl WhereClause {
    fn is_empty(&self) -> bool {
        match self {
            WhereClause::Single(s) => s.is_empty(),
            WhereClause::Multiple(v) => v.is_empty() || v.iter().all(|s| s.is_empty()),
        }
    }
     fn get_single(&self) -> Option<&str> {
         match self {
             WhereClause::Single(s) => Some(s),
             WhereClause::Multiple(_) => None, // Or maybe first? Depends on logic
         }
     }
      fn get_multiple(&self) -> Option<&Vec<String>> {
         match self {
             WhereClause::Single(_) => None,
             WhereClause::Multiple(v) => Some(v),
         }
     }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeTarget {
    list: String,
    url: Option<String>, // Optional URL override for the list
    #[serde(flatten)] // Include all other GetListItemsOptions fields
    options: Box<GetListItemsOptions>, // Use Box<GetListItemsOptions> to avoid recursion limits? Simpler: just the relevant fields
}

// Structure to hold data from the parent list during a join
#[derive(Debug, Clone)]
pub struct JoinData {
    // Key: String representation of joined field values from parent (e.g., "_value1_value2")
    // Value: Vec of parent list items matching that key
    indexed_data: HashMap<String, Vec<ListItem>>,
    // List of unique index keys encountered in the parent list
    index_keys: Vec<String>,
    // Parsed ON clause defining the relationship
    on_pairs: Vec<JoinFieldPair>,
    // Name/Alias of the parent list
    parent_alias: String,
     // Is this an outer join?
    outer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinOptions {
    pub list: String,
    pub url: Option<String>,
    pub on: Option<String>, // e.g., "'List1'.Field1 = 'List2'.Field2"
    pub on_lookup: Option<String>, // Optimized join on lookup field (points to parent ID)
    #[serde(default)]
    pub outer: bool, // Is this an outer join?
    // Recursive join
    pub join: Option<Box<JoinOptions>>,
    pub outerjoin: Option<Box<JoinOptions>>,
    // Include other GetListItemsOptions fields
    #[serde(flatten)]
    pub options: Box<GetListItemsOptions>, // Flatten fields, where, orderby etc.
}

fn default_true() -> bool { true }
fn default_page_limit() -> usize { 5000 } // Or some large number meaning "infinite pages" until data runs out

// Represents the main SharePoint List client object
pub struct SharePointList {
    list_id: String,
    base_url: Url, // Base URL of the SharePoint site
    client: Client, // HTTP client
}

// Result structure including the next page token
#[derive(Debug)]
pub struct GetListItemsResult {
   pub items: Vec<ListItem>,
   pub next_page_token: Option<String>,
}

impl SharePointList {
    pub fn new(list_id: String, base_url: Url, client: Client) -> Self {
        SharePointList { list_id, base_url, client }
    }

    #[async_recursion] // Allow recursive calls for paging/joins/merges
    pub async fn get(&self, mut options: GetListItemsOptions) -> Result<GetListItemsResult> {
        info!("Starting get for list '{}' with options: {:?}", self.list_id, options); // Use debug in production

        if self.list_id.is_empty() { return Err(SpSharpError::MissingListId); }

        // 1. Process Options & Defaults
        let list_alias = options.alias.clone().unwrap_or_else(|| self.list_id.clone());
        let mut current_fields: HashSet<String> = options.fields.iter().cloned().collect();
        let mut current_orderby = options.orderby.clone().unwrap_or_default();
        let mut current_where = options.where_clause.clone();
        let mut is_where_caml = options.where_caml;
        let is_paging_or_multiwhere = options.paging || matches!(options.where_clause, WhereClause::Multiple(_));

        // Clean next page token
        if let Some(token) = options.next_page_token.as_mut() {
            *token = token.replace('&', "&"); // Basic escaping
        }

        // --- Handle View ---
        if let Some(view_name_or_id) = &options.view {
            if !view_name_or_id.is_empty() {
                info!("Processing view: {}", view_name_or_id);
                 let ctx = ViewContext { list_id: &self.list_id, url: &self.base_url };
                let view_details = view::get_view_details(ctx, view_name_or_id, options.view_cache).await?;

                // Merge view fields
                current_fields.extend(view_details.fields);

                // Merge view OrderBy (append if user also provided)
                if let Some(view_orderby) = view_details.order_by {
                     if !current_orderby.is_empty() && !view_orderby.is_empty() {
                         current_orderby = format!("{},{}", current_orderby, view_orderby);
                     } else if !view_orderby.is_empty() {
                         current_orderby = view_orderby;
                     }
                 }

                 // Merge view Where (complex!) - Requires CAML understanding
                if let Some(view_where_caml) = view_details.where_caml {
                    let parsed_user_where = match ¤t_where {
                        WhereClause::Single(s) if !s.is_empty() => {
                            if is_where_caml { vec![s.clone()] }
                            else { vec![parse_where_to_caml(s, options.where_escape_char)?] }
                        },
                        WhereClause::Multiple(v) => {
                             if is_where_caml { v.clone() }
                             else {
                                 let mut parsed = Vec::new();
                                 for s in v {
                                     parsed.push(parse_where_to_caml(s, options.where_escape_char)?);
                                 }
                                 parsed
                             }
                        },
                        _ => Vec::new(), // Empty user where
                    };

                    let mut combined_wheres = Vec::new();
                    if parsed_user_where.is_empty() {
                         // Handle potential DateRangesOverlap moving (as per JS comment) - needs CAML parser
                         let processed_view_where = view_where_caml; // Simplified
                        combined_wheres.push(processed_view_where);
                    } else {
                        for user_w in parsed_user_where {
                            // Combine with <And> - Needs robust CAML combination logic
                            let combined = format!("<And>{}{}</And>", user_w, view_where_caml);
                            combined_wheres.push(combined);
                        }
                    }

                    current_where = if combined_wheres.len() == 1 {
                        WhereClause::Single(combined_wheres.remove(0))
                    } else {
                        WhereClause::Multiple(combined_wheres)
                    };
                    is_where_caml = true; // Resulting where is CAML
                 }
                 // If the original options triggered calendar, remember it
                 options.calendar_via_view = options.calendar;
                 options.calendar = false; // View settings override direct calendar option for query building
             }
             // Clear view option so it's not processed again in recursion
             options.view = None;
         }


        // --- Handle Multi-Where (Throttling Workaround) ---
        if let WhereClause::Multiple(wheres) = ¤t_where {
            if options.next_where.is_empty() && options.results.is_empty() { // First call for multi-where
                let mut remaining_wheres = wheres.clone();
                if remaining_wheres.is_empty() {
                     // Should not happen if Multiple is constructed correctly, but handle defensively
                     warn!("Empty multi-where clause received.");
                     current_where = WhereClause::Single("".to_string());
                     options.next_where = Vec::new();
                } else {
                     let first_where = remaining_wheres.remove(0);
                     options.original_where = Some(current_where.clone()); // Store original
                     current_where = WhereClause::Single(first_where);
                     options.next_where = remaining_wheres; // Store remaining for recursive calls
                }

            }
            // Note: Progress callback for multi-where needs handling within the loop/recursion below
        }

        // --- Prepare CAML Query Components ---

        // Fields (ViewFields)
        let fields_xml = current_fields
            .iter()
            .map(|f| format!(r#"<FieldRef Name="{}" />"#, f))
            .collect::<String>();

        // OrderBy
        let orderby_xml = if !current_orderby.is_empty() {
            let clauses = current_orderby
                .split(',')
                .map(|part| {
                    let trimmed = part.trim();
                    let mut parts = trimmed.split_whitespace();
                    let field = parts.next().unwrap_or("");
                    let direction = parts.next().unwrap_or("ASC").to_uppercase();
                    if field.is_empty() { return "".to_string(); }
                    format!(
                        r#"<FieldRef Name="{}" Ascending="{}" />"#,
                        field,
                        direction == "ASC"
                    )
                })
                .filter(|s| !s.is_empty())
                .collect::<String>();
             // Add attributes for SP2010 throttling workaround if needed
             let orderby_attrs = if options.use_index_for_orderby {
                 " UseIndexForOrderBy='TRUE' Override='TRUE'"
             } else {
                 ""
             };
             format!("<OrderBy{}>{}</OrderBy>", orderby_attrs, clauses)
        } else if options.calendar || options.calendar_via_view {
             // Default sort for calendar
            "<OrderBy><FieldRef Name=\"EventDate\" Ascending=\"TRUE\" /></OrderBy>".to_string()
        } else {
            "".to_string()
        };

        // GroupBy
        let groupby_xml = options
            .groupby
            .as_ref()
            .map(|g| {
                g.split(',')
                    .map(|field| format!(r#"<FieldRef Name="{}" />"#, field.trim()))
                    .collect::<String>()
            })
            .map(|fields| format!("<GroupBy Collapse=\"TRUE\">{}</GroupBy>", fields)) // Collapse? GroupLimit?
            .unwrap_or_default();


        // Where
        let where_inner_xml = match ¤t_where {
             WhereClause::Single(s) if !s.is_empty() => {
                 if is_where_caml { s.clone() }
                 else { parse_where_to_caml(s, options.where_escape_char)? }
             }
             // Multi-where case is handled by taking the first one above and recursing
             _ => "".to_string()
         };

        let mut final_where_inner = where_inner_xml;

        // Add Calendar DateRangesOverlap if needed
        if options.calendar || options.calendar_via_view {
            let cal_opts = options.calendar_options.get_or_insert_with(Default::default); // Ensure defaults
            let range_tag = match cal_opts.range {
                 CalendarRange::Month => "Month",
                 CalendarRange::Week => "Week",
                 CalendarRange::Day => "Day", // Assuming Day exists in CAML
             };
            let date_range_overlap = format!(
                 "<DateRangesOverlap>\
                    <FieldRef Name='EventDate' />\
                    <FieldRef Name='EndDate' />\
                    <FieldRef Name='RecurrenceID' />\
                    <Value Type='DateTime'><{} /></Value>\
                  </DateRangesOverlap>", range_tag
             );

             final_where_inner = if !final_where_inner.is_empty() {
                 format!("<And>{}{}</And>", final_where_inner, date_range_overlap)
             } else {
                 date_range_overlap
             };
        }

        let where_xml = if !final_where_inner.is_empty() {
            // Apply where_fct if implemented
            // let processed_where = options.where_fct.map_or(final_where_inner, |f| f(final_where_inner));
            format!("<Where>{}</Where>", final_where_inner)
        } else {
            "".to_string()
        };


        // Query Options
        let mut query_options_xml_builder = String::new();
        if let Some(qo) = &options.query_options {
            query_options_xml_builder = qo.clone(); // User override
        } else {
            query_options_xml_builder.push_str(&format!(
                "<DateInUtc>{}</DateInUtc>",
                if options.date_in_utc { "True" } else { "False" }
            ));
            query_options_xml_builder.push_str(&format!(
                "<Paging ListItemCollectionPositionNext=\"{}\" />",
                options.next_page_token.as_deref().unwrap_or("")
            ));
             query_options_xml_builder.push_str("<IncludeAttachmentUrls>True</IncludeAttachmentUrls>");
            if !current_fields.is_empty() {
                 query_options_xml_builder.push_str("<IncludeMandatoryColumns>False</IncludeMandatoryColumns>");
             }
             query_options_xml_builder.push_str(&format!(
                 "<ExpandUserField>{}</ExpandUserField>",
                 if options.expand_user_field { "True" } else { "False" }
             ));

            // Handle Folder Options
             if let Some(folder_opts) = &mut options.folder_options {
                 let root_folder_path = match &folder_opts.root_folder {
                     Some(rf) => rf.clone(),
                     None => {
                         info!("Folder options specified without rootFolder, fetching list info...");
                          let info_ctx = InfoContext { list_id: &self.list_id, url: &self.base_url };
                         let list_info = info::get_list_info(info_ctx).await?;
                         folder_opts.root_folder = Some(list_info.root_folder.clone());
                         list_info.root_folder
                     }
                 };
                 let view_scope = match folder_opts.show {
                    FolderShow::FilesAndFoldersRecursive => "RecursiveAll",
                    FolderShow::FilesOnlyInFolder => "FilesOnly",
                    FolderShow::FilesAndFoldersInFolder => "", // Default seems empty
                    FolderShow::FilesOnlyRecursive => "Recursive", // Default in JS? Check MSDN
                 };
                 if !view_scope.is_empty() {
                    query_options_xml_builder.push_str(&format!(r#"<ViewAttributes Scope="{}"/>"#, view_scope));
                 }
                 if !folder_opts.path.is_empty() {
                      // Ensure path doesn't start/end with / and join correctly
                     let clean_path = folder_opts.path.trim_matches('/');
                     let full_folder_path = format!("{}/{}", root_folder_path.trim_end_matches('/'), clean_path);
                     query_options_xml_builder.push_str(&format!("<Folder>{}</Folder>", full_folder_path));
                 }
            } else {
                 // Default view attributes if no folder options? JS adds <ViewAttributes Scope="Recursive"/>
                  query_options_xml_builder.push_str(r#"<ViewAttributes Scope="Recursive"/>"#); // Check if this is always desired
             }

            // Handle Calendar Options
             if options.calendar || options.calendar_via_view {
                 let cal_opts = options.calendar_options.get_or_insert_with(Default::default);
                 query_options_xml_builder.push_str(&format!(
                     "<CalendarDate>{}</CalendarDate>",
                      to_sp_date_string(&cal_opts.reference_date) // Format date correctly
                 ));
                  query_options_xml_builder.push_str("<RecurrencePatternXMLVersion>v3</RecurrencePatternXMLVersion>");
                  query_options_xml_builder.push_str(&format!(
                      "<ExpandRecurrence>{}</ExpandRecurrence>",
                      if cal_opts.split_recurrence { "TRUE" } else { "FALSE" } // CAML usually uses uppercase bools
                 ));
             }
        }


        // --- Construct SOAP Body ---
        let body_content = format!(
            r#"<listName>{}</listName>
               <viewName>{}</viewName>
               <query><Query>{}{}{}</Query></query>
               <viewFields><ViewFields Properties='True'>{}</ViewFields></viewFields>
               <rowLimit>{}</rowLimit>
               <queryOptions><QueryOptions>{}</QueryOptions></queryOptions>"#,
            self.list_id,
            "", // viewName is usually GUID, handled by GetListAndView implicitly? Or use view_details.id if available? JS passes empty.
            where_xml,
            groupby_xml,
            orderby_xml,
            fields_xml,
            if options.paging { options.rowlimit.max(1) } else { options.rowlimit }, // Ensure rowlimit > 0 if paging
            query_options_xml_builder
        );

        let soap_body = build_soap_body("GetListItems", &body_content);

        // --- Make HTTP Request ---
        let request_url = self.base_url.join("_vti_bin/Lists.asmx")?;
        info!("Sending GetListItems request to {}", request_url);
        debug!("SOAP Body:\n{}", soap_body);

        let response = self.client
            .post(request_url)
            .header("Content-Type", "text/xml; charset=utf-8")
            .header("SOAPAction", "http://schemas.microsoft.com/sharepoint/soap/GetListItems")
            .body(soap_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            warn!("GetListItems failed: Status={}, Body={}", status, error_text);
            // TODO: Parse SOAP Fault for better error message
             return Err(SpSharpError::SharePointError {
                 code: status.to_string(),
                 message: error_text,
             });
        }

        let response_text = response.text().await?;
        debug!("SOAP Response:\n{}", response_text);

        // --- Parse XML Response ---
        let mut reader = Reader::from_str(&response_text);
        reader.trim_text(true);
        let mut buf = Vec::new();
        let mut current_item: Option<ListItem> = None;
        let mut parsed_items: Vec<ListItem> = Vec::new();
        let mut response_next_page_token: Option<String> = None;

        loop {
            match reader.read_event_mut(&mut buf)? {
                Event::Start(ref e) | Event::Empty(ref e) => {
                    match e.name().as_ref() {
                        b"z:row" | b"row" => { // Handle different namespaces/tags
                            current_item = Some(ListItem::new());
                            for attr_result in e.attributes() {
                                let attr = attr_result?;
                                let key = std::str::from_utf8(attr.key.as_ref())?.to_string();
                                // Store as "FieldName" removing "ows_" prefix
                                if let Some(stripped_key) = key.strip_prefix("ows_") {
                                    let value = attr.decode_and_unescape_value(&reader)?.to_string();
                                     if let Some(item) = current_item.as_mut() {
                                         // Add list alias prefix if needed (but not for joins yet)
                                         let final_key = if options.show_list_in_attribute && !options.is_join_child {
                                             format!("{}.{}", list_alias, stripped_key)
                                         } else {
                                             stripped_key.to_string()
                                         };
                                         item.insert(final_key, Some(value));
                                     }
                                }
                            }
                             // If it's an empty tag, commit immediately
                             if matches!(reader.read_event_mut(&mut buf)?, Event::End(_)) || matches!(e, BytesStart { .. } if e.name().as_ref() == b"z:row" || e.name().as_ref() == b"row") {
                                 if let Some(item) = current_item.take() {
                                     parsed_items.push(item);
                                 }
                             }
                        }
                        b"rs:data" | b"data" => { // Check for paging token
                            for attr_result in e.attributes() {
                                let attr = attr_result?;
                                if attr.key.as_ref() == b"ListItemCollectionPositionNext" {
                                    let token = attr.decode_and_unescape_value(&reader)?.to_string();
                                    if !token.is_empty() {
                                         response_next_page_token = Some(token);
                                     }
                                }
                            }
                        }
                        _ => (),
                    }
                }
                 Event::End(ref e) => {
                     match e.name().as_ref() {
                        b"z:row" | b"row" => {
                            if let Some(item) = current_item.take() {
                                parsed_items.push(item);
                            }
                        }
                        _ => (),
                     }
                 }
                Event::Eof => break,
                _ => (),
            }
            buf.clear();
        }


        // --- Combine results (for paging/multi-where) ---
        let mut combined_results = options.results; // Take accumulated results
        combined_results.extend(parsed_items);


        // --- Handle Paging Recursion ---
        if options.paging && options.page > 1 { // Check if more pages are requested
             if let Some(next_token) = &response_next_page_token {
                 info!("Paging: Got next page token, requesting next page.");
                 if let Some(progress_fn) = &options.progress {
                     progress_fn(combined_results.len(), None); // Progress update
                 }
                 let mut next_options = options.clone(); // Clone options for next request
                 next_options.next_page_token = Some(clean_string(next_token));
                 next_options.page -= 1; // Decrement page counter
                 next_options.results = combined_results; // Pass accumulated results
                 return self.get(next_options).await; // Recursive call
            } else {
                 info!("Paging: No next page token received, assuming end of list.");
            }
        }


        // --- Handle Multi-Where Recursion ---
        if !options.next_where.is_empty() {
            info!("Multi-Where: Processing next where clause.");
             if let Some(progress_fn) = &options.progress {
                let total_wheres = match &options.original_where {
                    Some(WhereClause::Multiple(v)) => v.len(),
                    _ => 1, // Should not happen
                };
                 let completed_wheres = total_wheres.saturating_sub(options.next_where.len());
                progress_fn(completed_wheres, Some(total_wheres)); // Progress update
            }
            let mut next_options = options.clone(); // Clone options
            let next_where_clause = next_options.next_where.remove(0);
             next_options.where_clause = WhereClause::Single(next_where_clause);
             next_options.next_page_token = None; // Reset paging for next where segment
             next_options.results = combined_results; // Pass accumulated results
             // Make sure where_caml is set correctly based on original options
             next_options.where_caml = options.original_where.as_ref().map_or(false, |w| matches!(w, WhereClause::Multiple(_)) && options.where_caml);

            return self.get(next_options).await; // Recursive call
        }


        // --- Final Processing (Joins/Merges happen *after* all base data is fetched) ---
         let mut final_items = combined_results;

        // --- Handle Joins ---
         let mut effective_join_options: Option<(Box<JoinOptions>, bool)> = None; // (JoinOptions, is_outer)
         if let Some(join_opts) = options.join {
            effective_join_options = Some((join_opts, false));
         } else if let Some(outer_join_opts) = options.outerjoin {
            effective_join_options = Some((outer_join_opts, true));
         }

        if let Some((join_opts_boxed, is_outer_from_option)) = effective_join_options {
             let join_opts = *join_opts_boxed; // Deref the box
             let is_outer = is_outer_from_option || join_opts.outer; // Check both outerjoin keyword and explicit flag
             info!("Processing {} join with list '{}'", if is_outer { "OUTER" } else { "INNER" }, join_opts.list);

             if !final_items.is_empty() {
                 // 1. Parse ON clause
                 let on_clause = join_opts.on.as_deref().or_else(|| join_opts.on_lookup.as_ref().map(|lkp| {
                      // Construct ON clause if only on_lookup is provided
                      // Assumes lookup field on child points to parent's ID
                      format!("'{}'.{} = '{}'.ID",
                          join_opts.options.alias.as_deref().unwrap_or(&join_opts.list),
                          lkp,
                          list_alias)
                  })).ok_or(SpSharpError::InvalidJoinOnClause)?;

                let on_pairs = parse_on_clause(on_clause)?;
                if on_pairs.is_empty() { return Err(SpSharpError::InvalidJoinOnClause); }

                // 2. Index data from *this* list (the "parent" in this join step)
                let mut indexed_parent_data: HashMap<String, Vec<ListItem>> = HashMap::new();
                let mut parent_index_keys: Vec<String> = Vec::new();
                let mut join_lookup_values_for_where: HashSet<String> = HashSet::new(); // For optimizing child query with IN

                 // Determine which side of the pairs refers to the *parent* (current list)
                 // This assumes parse_on_clause gives consistent ordering or we check both sides
                 let parent_refers_to = |p: &JoinFieldPair| -> Option<&str> {
                    if p.list1_name == list_alias { Some(&p.list1_field) }
                    else if p.list2_name == list_alias { Some(&p.list2_field) }
                    else { None }
                 };
                 let child_refers_to = |p: &JoinFieldPair| -> Option<&str> {
                     let child_list_name = join_opts.options.alias.as_deref().unwrap_or(&join_opts.list);
                     if p.list1_name == child_list_name { Some(&p.list1_field) }
                     else if p.list2_name == child_list_name { Some(&p.list2_field) }
                     else { None }
                 };


                for item in &final_items {
                    let mut index_key_parts = Vec::new();
                    let mut valid_key = true;
                    for pair in &on_pairs {
                        // Find the field name for *this* list in the pair
                         if let Some(parent_field_name) = parent_refers_to(pair) {
                             // Need to handle potential list alias prefix if showListInAttribute was true before join
                             let lookup_key1 = format!("{}.{}", list_alias, parent_field_name);
                             let lookup_key2 = parent_field_name.to_string();

                             let value = item.get(&lookup_key1).or_else(|| item.get(&lookup_key2)).flatten();

                             // Use get_lookup_id to extract ID part if it's a lookup format
                            let id_part = get_lookup_id(value.map(|s| s.as_str())).unwrap_or_else(|| value.cloned().unwrap_or_default());

                            if id_part.is_empty() {
                                // Handle cases where join field is empty - might depend on INNER/OUTER logic
                                // valid_key = false; break; // For INNER join, skip if key part is missing?
                            }
                            index_key_parts.push(id_part);

                            // If using on_lookup optimization, collect the IDs
                             if join_opts.on_lookup.is_some() && parent_field_name == "ID" {
                                if let Some(id_val) = item.get("ID").flatten() {
                                     join_lookup_values_for_where.insert(format!("~{}", id_val)); // Using ~ prefix as in JS example
                                 }
                             } else if let Some(child_field_name) = child_refers_to(&pair) {
                                 // Check if child field is the lookup and parent is ID
                                 if join_opts.on_lookup.as_deref() == Some(child_field_name) && parent_field_name == "ID" {
                                     if let Some(id_val) = item.get("ID").flatten() {
                                         join_lookup_values_for_where.insert(format!("~{}", id_val));
                                     }
                                 }
                                 // Check if the on clause implies a lookup without explicit on_lookup
                                  else if child_field_name == join_opts.on_lookup.as_deref().unwrap_or("") && parent_field_name == "ID" {
                                     if let Some(id_val) = item.get("ID").flatten() {
                                        join_lookup_values_for_where.insert(format!("~{}", id_val));
                                     }
                                 }
                             }

                        } else {
                            warn!("ON clause pair doesn't seem to reference parent list '{}': {:?}", list_alias, pair);
                            valid_key = false; break;
                        }
                    }

                    if valid_key {
                        let index_key = format!("_{}", index_key_parts.join("_")); // Construct key like JS
                         if !indexed_parent_data.contains_key(&index_key) {
                             parent_index_keys.push(index_key.clone());
                         }
                         // Clone the item - potentially expensive, maybe Rc?
                         // Prefix parent fields *before* adding to join_data if not already prefixed
                         let mut prefixed_item = ListItem::new();
                         for (k,v) in item.iter() {
                            // Avoid double prefixing if showListInAttribute was true
                             if !k.contains('.') {
                                 prefixed_item.insert(format!("{}.{}", list_alias, k), v.clone());
                             } else {
                                 prefixed_item.insert(k.clone(), v.clone());
                             }
                         }
                        indexed_parent_data.entry(index_key).or_default().push(prefixed_item);
                    }
                }

                // 3. Prepare options for child list call
                let mut child_options = *join_opts.options; // Get options specific to the child join
                child_options.alias = child_options.alias.or_else(|| Some(join_opts.list.clone())); // Ensure alias for child

                // Add WHERE clause based on collected lookup values if applicable (onLookup optimization)
                 let lookup_field_for_where = join_opts.on_lookup.as_deref();
                 if let Some(lookup_field) = lookup_field_for_where {
                     if !join_lookup_values_for_where.is_empty() {
                         let max_in_clause = 500; // Adjust based on testing SP limits
                         let chunks = join_lookup_values_for_where.iter().chunks(max_in_clause);
                         let mut where_parts: Vec<String> = Vec::new();

                        for chunk in &chunks {
                             let values_str = chunk.map(|v| format!(r#""{}""#, v)).join(",");
                             where_parts.push(format!(r#"{} IN [{}]"#, lookup_field, values_str));
                        }

                        if where_parts.len() < 10 { // Heuristic limit for OR complexity
                             let combined_lookup_where = format!("({})", where_parts.join(" OR "));

                            // Combine with existing child WHERE
                             child_options.where_clause = match child_options.where_clause {
                                 WhereClause::Single(ref s) if !s.is_empty() => {
                                    WhereClause::Single(format!("({}) AND ({})", combined_lookup_where, s))
                                 },
                                 WhereClause::Multiple(ref v) => {
                                     // Combine with each part? Or just wrap the whole thing? Safer to wrap.
                                     let existing_multi = format!("({})", v.join(") OR ("));
                                     WhereClause::Single(format!("({}) AND ({})", combined_lookup_where, existing_multi))
                                 },
                                 _ => WhereClause::Single(combined_lookup_where),
                             };
                             child_options.where_caml = false; // Force re-parsing as we used SQL-like IN
                         } else {
                            warn!("Large number of IN clauses generated for join ({}), might hit SP limits. Consider filtering parent list first or using paging on child.", where_parts.len());
                             // Fallback: Don't add the optimized WHERE, rely on post-fetch filtering.
                             // Or implement paging on the child call.
                             child_options.paging = true; // Force paging on child if WHERE is too complex
                         }

                        // Ensure the lookup field is requested from the child
                         if !child_options.fields.iter().any(|f| f == lookup_field) {
                            child_options.fields.push(lookup_field.to_string());
                         }
                     } else {
                         // No parent items matched or lookup field wasn't ID? Handle empty join result.
                         warn!("Join with onLookup='{}' resulted in no values to query in child list.", lookup_field);
                         final_items.clear(); // Inner join with no matching keys = empty result
                         // Outer join needs different handling below
                     }
                 }


                // 4. Create JoinData to pass to child
                let join_data_to_pass = JoinData {
                    indexed_data: indexed_parent_data,
                    index_keys: parent_index_keys,
                    on_pairs: on_pairs.clone(), // Pass the parsed rules
                    parent_alias: list_alias.clone(),
                     outer: is_outer,
                };
                child_options.join_data = Some(join_data_to_pass);
                child_options.is_join_child = true; // Mark as child

                // 5. Make recursive call for the child list
                let child_list_url = join_opts.url.as_ref().map_or_else(
                    || Ok(self.base_url.clone()), // Use parent URL if not specified
                     |url_str| self.base_url.join(url_str) // Resolve relative to parent base
                )?;
                let child_sp_list = SharePointList::new(join_opts.list.clone(), child_list_url, self.client.clone());

                 // Handle nested joins within the child options
                 child_options.join = join_opts.join;
                 child_options.outerjoin = join_opts.outerjoin;

                let joined_result = child_sp_list.get(child_options).await?;
                final_items = joined_result.items; // The result from the child call IS the joined data


             } else {
                 // Parent list was empty, so join result is empty (unless outer join logic applies differently)
                 info!("Join skipped: Parent list '{}' returned no items.", self.list_id);
                 final_items.clear();
             }

        }
         // Handle join result when current call *is* the child
         else if let Some(join_ctx) = options.join_data {
             info!("Processing as join child, merging with parent '{}' data.", join_ctx.parent_alias);
             let mut joined_results: Vec<ListItem> = Vec::new();
             let mut parent_indices_found: HashSet<usize> = HashSet::new(); // Track used parent indices for outer join

             // Determine child fields based on ON clause
             let child_alias = options.alias.as_deref().unwrap_or(&self.list_id);
             let child_refers_to = |p: &JoinFieldPair| -> Option<&str> {
                 if p.list1_name == child_alias { Some(&p.list1_field) }
                 else if p.list2_name == child_alias { Some(&p.list2_field) }
                 else { None }
             };

             for child_item in &final_items { // final_items here are the rows from the child list itself
                 let mut index_key_parts = Vec::new();
                 let mut valid_key = true;
                 for pair in &join_ctx.on_pairs {
                     if let Some(child_field_name) = child_refers_to(pair) {
                          // Child item keys won't be prefixed yet
                         let value = child_item.get(child_field_name).flatten();
                         let id_part = get_lookup_id(value.map(|s| s.as_str())).unwrap_or_else(|| value.cloned().unwrap_or_default());
                         if id_part.is_empty() {
                            // valid_key = false; break; // Skip if key part missing for inner join?
                         }
                         index_key_parts.push(id_part);
                     } else {
                         warn!("ON clause pair doesn't seem to reference child list '{}': {:?}", child_alias, pair);
                         valid_key = false; break;
                     }
                 }

                 if valid_key {
                     let index_key = format!("_{}", index_key_parts.join("_"));
                     // Look up this key in the parent data passed via join_ctx
                     if let Some(parent_items) = join_ctx.indexed_data.get(&index_key) {
                          // Mark parent index as found (need mapping from key to original index)
                         if let Some(idx) = join_ctx.index_keys.iter().position(|k| k == &index_key) {
                            parent_indices_found.insert(idx);
                         }

                         // Merge child with each matching parent
                         for parent_item in parent_items {
                             let mut merged_item = parent_item.clone(); // Start with parent (already prefixed)
                             // Add child fields, prefixed
                             for (key, value) in child_item.iter() {
                                 merged_item.insert(format!("{}.{}", child_alias, key), value.clone());
                             }
                             joined_results.push(merged_item);
                         }
                     }
                     // If inner join and no match found, child_item is dropped implicitly
                 }
             }

            // Outer Join Handling: Add parent rows that had no match
             if join_ctx.outer {
                 info!("Handling outer join for parent '{}'", join_ctx.parent_alias);
                 for (idx, key) in join_ctx.index_keys.iter().enumerate() {
                     if !parent_indices_found.contains(&idx) {
                         if let Some(unmatched_parent_items) = join_ctx.indexed_data.get(key) {
                             for parent_item in unmatched_parent_items {
                                 // Add parent item, potentially padding child fields with None
                                 let mut outer_item = parent_item.clone();
                                 // How to know expected child fields? Use options.fields? Risky.
                                 // Simplest: Just don't add any child fields.
                                 warn!("Outer join: Parent item with key '{}' had no match in child '{}'. Child fields will be missing.", key, child_alias);
                                 joined_results.push(outer_item);
                             }
                         }
                     }
                 }
             }

             final_items = joined_results; // Replace child items with merged results
         }


        // --- Handle Merge ---
         if let Some(merge_targets) = options.merge {
             info!("Merging results with {} other list(s).", merge_targets.len());
             let mut collected_merge_data = options.merge_data; // Take accumulated data

             // Add items from the current list to the collection, adding Source info
             let current_source = SourceInfo { list: self.list_id.clone(), url: self.base_url.to_string() };
             collected_merge_data.extend(final_items.into_iter().map(|item| (item, current_source.clone())));

             if !merge_targets.is_empty() {
                 let mut remaining_targets = merge_targets;
                 let next_target_def = remaining_targets.remove(0); // Process one merge target

                 let mut next_options = *next_target_def.options;
                 next_options.merge_data = collected_merge_data; // Pass accumulated data
                 next_options.merge = Some(remaining_targets); // Pass remaining targets

                 let target_list_url = next_target_def.url.as_ref().map_or_else(
                     || Ok(self.base_url.clone()),
                      |url_str| self.base_url.join(url_str)
                 )?;
                 let target_sp_list = SharePointList::new(next_target_def.list.clone(), target_list_url, self.client.clone());

                 // Recursive call for the merge target
                 return target_sp_list.get(next_options).await;
             } else {
                 // Last merge step, convert collected data back to Vec<ListItem> adding the Source field
                  final_items = collected_merge_data.into_iter().map(|(mut item, source)| {
                      // Serialize source and add it. Using JSON string for simplicity.
                      let source_json = serde_json::to_string(&source).unwrap_or_default();
                      item.insert("Source".to_string(), Some(source_json));
                      item
                  }).collect();
             }
         }

        // --- Final Result ---
        info!("Get successful for list '{}'. Returning {} items.", self.list_id, final_items.len());
        if is_paging_or_multiwhere {
             // Call progress one last time if it's the end of a multi-step process
             if let Some(progress_fn) = &options.progress {
                  if options.paging {
                      progress_fn(final_items.len(), None); // Total items loaded
                  } else if let Some(WhereClause::Multiple(v)) = options.original_where {
                      progress_fn(v.len(), Some(v.len())); // Final progress for multi-where
                  }
              }
         }

        Ok(GetListItemsResult {
            items: final_items,
            next_page_token: response_next_page_token,
        })
    }
}

// --- Helper Functions (Example - Needs proper implementation) ---
mod defaults {
     use super::*;
     pub fn default_calendar() -> CalendarOptions { CalendarOptions { split_recurrence: true, reference_date: Utc::now(), range: CalendarRange::Month } }
 }


 #[cfg(test)]
 mod tests {
     // Add tests here using mockall for utils and reqwest mocks if needed
 }