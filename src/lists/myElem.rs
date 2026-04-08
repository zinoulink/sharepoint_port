use std::collections::HashMap;

/**
 * @ignore
 * @description This struct is a Rust equivalent of the JavaScript class
 *   designed to extend an element for certain cases, particularly when
 *   working with SharePoint list items retrieved via `$SP().get`.
 *   It wraps a collection of attributes, typically parsed from a SharePoint
 *   XML response, where keys might include "ows_" prefixes.
 **/
pub struct MyElem {
    // In the original JavaScript, `elem` is likely a DOM-like object
    // (e.g., an XML element) that provides `getAttribute` and `attributes` methods.
    // In Rust, we'll represent its attributes as a HashMap for a direct translation
    // of the JavaScript behavior. Keys in this HashMap are expected to be the
    // raw attribute names, potentially including "ows_" prefixes as they appear
    // in SharePoint XML responses (e.g., "ows_Title", "ID").
    mynode_attributes: HashMap<String, String>,
    single_list: bool,
}

impl MyElem {
    /// Creates a new `MyElem` instance.
    ///
    /// # Arguments
    ///
    /// * `elem_attributes` - A `HashMap` representing the attributes of an XML element.
    ///   Keys should be the raw attribute names (e.g., "ows_Title", "ID"), and values
    ///   are their string representations.
    pub fn new(elem_attributes: HashMap<String, String>) -> Self {
        Self {
            mynode_attributes: elem_attributes,
            single_list: true, // Directly translates `this.singleList = true;`
        }
    }

    /// Retrieves an attribute value from the wrapped element.
    ///
    /// This method mimics the JavaScript `getAttribute` behavior. It constructs
    /// the attribute key by prepending "ows_" and removing spaces from the input `id`,
    /// then looks up this key in the internal attribute map.
    ///
    /// # Arguments
    ///
    /// * `id` - The logical name of the attribute (e.g., "Title", "ID").
    ///
    /// # Returns
    ///
    /// An `Option<&String>` containing the attribute's value if found, otherwise `None`.
    pub fn get_attribute(&self, id: &str) -> Option<&String> {
        // Directly translates `return this.mynode.getAttribute("ows_"+id.replace(/ /g,""));`
        let key_to_find = format!("ows_{}", id.replace(' ', ""));
        self.mynode_attributes.get(&key_to_find)
    }

    /// Returns a reference to all raw attributes of the wrapped element.
    ///
    /// This directly translates `return this.mynode.attributes;` by returning a reference
    /// to the internal `HashMap` of attributes.
    ///
    /// # Returns
    ///
    /// A reference to a `HashMap<String, String>` containing all attributes.
    pub fn get_attributes(&self) -> &HashMap<String, String> {
        &self.mynode_attributes
    }
}