use once_cell::sync::Lazy; // For efficient static regex compilation
use regex::Regex;
use std::borrow::Cow; // To handle string replacements efficiently

// Pre-compile Regex instances for performance using once_cell::sync::Lazy
// This avoids compiling the regex on every function call.
static RE_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(string|float|datetime);#?").unwrap());
static RE_DATE_TIME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\d{4}-\d{2}-\d{2}) 00:00:00$").unwrap());
static RE_INTERNAL_ID_SEP: Lazy<Regex> = Lazy::new(|| Regex::new(r";#-?\d+;#").unwrap());
static RE_LEADING_ID_SEP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^-?\d+;#").unwrap());
static RE_EDGE_SEP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^;#|;#$").unwrap());
static RE_INTERNAL_SEP: Lazy<Regex> = Lazy::new(|| Regex::new(r";#").unwrap());


/// Cleans a string potentially returned by SharePoint GET requests.
///
/// Removes prefixes like `number;#`, `string;#`, etc., handles list separators,
/// and cleans date formats. Null or undefined inputs result in an empty string.
///
/// Corresponds to the JavaScript function `$SP().cleanResult`.
///
/// # Arguments
///
/// * `input_str` - An optional string slice (`&str`) to clean. Mimics JS `null`/`undefined`. If `None`, returns `""`.
/// * `separator` - An optional string slice (`&str`) to use as a separator for list-like items. Defaults to ";".
///
/// # Returns
///
/// A `String` containing the cleaned result.
///
/// # Examples
///
/// ```
/// // Assuming this function is in scope, e.g., `use your_crate::clean_result;`
/// # use your_crate::clean_result; // Replace your_crate with actual crate name
/// assert_eq!(clean_result(Some("15;#Paul"), None), "Paul");
/// assert_eq!(clean_result(Some("string;#Paul"), None), "Paul");
/// assert_eq!(clean_result(Some("string;#"), None), "");
/// assert_eq!(clean_result(Some(";#Paul;#Jacques;#Aymeric;#"), None), "Paul;Jacques;Aymeric");
/// assert_eq!(clean_result(Some(";#Paul;#Jacques;#Aymeric;#"), Some(", ")), "Paul, Jacques, Aymeric");
/// assert_eq!(clean_result(Some("2022-01-19 00:00:00"), None), "2022-01-19");
/// assert_eq!(clean_result(None, None), ""); // Test null input
/// assert_eq!(clean_result(Some(""), None), ""); // Test empty string input
/// assert_eq!(clean_result(Some("float;#123.45"), None), "123.45");
/// assert_eq!(clean_result(Some("datetime;#2023-10-27 00:00:00"), None), "2023-10-27"); // Combined prefix and date
/// ```
pub fn clean_result(input_str: Option<&str>, separator: Option<&str>) -> String {
    // Handle null/undefined equivalent
    let s = match input_str {
        Some(s) => s,
        None => return String::new(), // Return empty string if input is None
    };

    // If the input string is empty, return empty string immediately
    if s.is_empty() {
        return String::new();
    }

    // Set default separator if not provided
    let separator = separator.unwrap_or(";");

    // Apply the cleaning steps sequentially.
    // Regex::replace/replace_all return Cow<str>, which avoids allocation
    // if no replacement is made. We only convert to String at the very end,
    // or when a replacement forces an allocation.
    // The order matters and mimics the JS implementation.

    // 1. Remove type prefixes: /^(string;|float;|datetime;)#?/,""
    let cleaned: Cow<str> = RE_PREFIX.replace(s, "");

    // 2. Clean date format: /^(\d{4}-\d{2}-\d{2}) 00:00:00$/, "$1"
    let cleaned: Cow<str> = RE_DATE_TIME.replace(&cleaned, "$1");

    // 3. Replace internal ID separators: /;#-?[0-9]+;#/g, separator
    //    Need replace_all because of the 'g' flag equivalent.
    let cleaned: Cow<str> = RE_INTERNAL_ID_SEP.replace_all(&cleaned, separator);

    // 4. Remove leading ID separator: /^-?[0-9]+;#/, ""
    //    Only needs replace (first match) due to '^' anchor.
    let cleaned: Cow<str> = RE_LEADING_ID_SEP.replace(&cleaned, "");

    // 5. Remove leading/trailing separators: /^;#|;#$/g, ""
    //    Needs replace_all because the pattern has an alternation '|'.
    let cleaned: Cow<str> = RE_EDGE_SEP.replace_all(&cleaned, "");

    // 6. Replace remaining internal separators: /;#/g, separator
    //    Needs replace_all because of the 'g' flag equivalent.
    let cleaned: Cow<str> = RE_INTERNAL_SEP.replace_all(&cleaned, separator);

    // Convert the final Cow<str> into an owned String
    cleaned.into_owned()
}

// Unit tests module
#[cfg(test)]
mod tests {
    use super::*; // Import the function from the parent module

    // Helper macro for cleaner tests (optional)
    macro_rules! test_clean {
        ($name:ident, $input:expr, $sep:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(clean_result($input, $sep), $expected);
            }
        };
    }

    test_clean!(test_id_value, Some("15;#Paul"), None, "Paul");
    test_clean!(test_string_prefix, Some("string;#Paul"), None, "Paul");
    test_clean!(test_string_prefix_empty, Some("string;#"), None, "");
    test_clean!(
        test_list_default_sep,
        Some(";#Paul;#Jacques;#Aymeric;#"),
        None,
        "Paul;Jacques;Aymeric"
    );
    test_clean!(
        test_list_custom_sep,
        Some(";#Paul;#Jacques;#Aymeric;#"),
        Some(", "),
        "Paul, Jacques, Aymeric"
    );
    test_clean!(
        test_date_time,
        Some("2022-01-19 00:00:00"),
        None,
        "2022-01-19"
    );
    test_clean!(test_null_input, None, None, "");
    test_clean!(test_empty_string_input, Some(""), None, "");
    test_clean!(test_float_prefix, Some("float;#123.45"), None, "123.45");
    test_clean!(
        test_datetime_prefix_and_clean,
        Some("datetime;#2023-10-27 00:00:00"),
        None,
        "2023-10-27"
    );
    test_clean!(
        test_list_with_ids_default_sep,
        Some("1;#Value1;#2;#Value2;#"),
        None,
        "Value1;Value2"
    );
    test_clean!(
        test_list_with_ids_custom_sep,
        Some("1;#Value1;#2;#Value2;#"),
        Some(" | "),
        "Value1 | Value2"
    );
    test_clean!(
        test_list_with_negative_id,
        Some("-1;#NegativeValue;#"),
        None,
        "NegativeValue"
    );
    test_clean!(
        test_no_cleaning_needed,
        Some("Just a regular string"),
        None,
        "Just a regular string"
    );
     test_clean!(
        test_only_separators,
        Some(";#;#;#"),
        None,
        ""
    );
     test_clean!(
        test_only_separators_custom_sep,
        Some(";#;#;#"),
        Some(","),
        "," // ;# -> , -> result is "," after edge trimming
    ); // Note: This behavior matches the JS sequential replaces
     test_clean!(
        test_tricky_separators,
        Some(";#Value1;#;#Value2;#"), // two separators in middle
        None,
        "Value1;;Value2" // Each ';#' becomes ';'
    );
     test_clean!(
        test_tricky_separators_custom,
        Some(";#Value1;#;#Value2;#"), // two separators in middle
        Some("|"),
        "Value1||Value2" // Each ';#' becomes '|'
    );
     test_clean!(
        test_id_only,
        Some("15;#"),
        None,
        ""
    );
}

// Placeholder main for compilation if needed, replace `your_crate` in examples
// with the actual crate name defined in Cargo.toml if you build this as a library.
// If this *is* the main binary crate, you can use `crate::` or `super::` in tests.
// For the example doc tests, replace `your_crate` with your crate's name.
// We define a dummy module `your_crate` here just for the doc test example to compile.
#[cfg(test)]
mod your_crate {
   pub use super::clean_result;
}
fn main() {
    println!("Testing clean_result:");
    println!("'15;#Paul' -> '{}'", clean_result(Some("15;#Paul"), None));
    println!("';#Paul;#Jacques;#' -> '{}'", clean_result(Some(";#Paul;#Jacques;#"), Some(", ")));
    println!("'2022-01-19 00:00:00' -> '{}'", clean_result(Some("2022-01-19 00:00:00"), None));
    println!("None -> '{}'", clean_result(None, None));

}