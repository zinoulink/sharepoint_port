use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value as JsonValue;
use crate::utils::clean_string; // Assuming this exists based on context

static RE_OP_SPACE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\s+)?(=|~=|<=|>=|~<>|<>|<|>| LIKE | IN )(\s+)?").unwrap()
});
static RE_EMPTY_STR: Lazy<Regex> = Lazy::new(|| Regex::new(r#""|''"#).unwrap());
static RE_EQ_EQ: Lazy<Regex> = Lazy::new(|| Regex::new(r"==").unwrap());
static RE_NULL_IN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\w+)\s+IN\s+\[([^\[]+,)?Null(,[^\]]+)?\]").unwrap()
});
static RE_NULL_IN_CLEAN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\[([^\[]+,)?Null(,[^\]]+)?\]").unwrap()
});
static RE_NULL_IN_COMMAS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\[),|(,),|,(\])").unwrap()
});
static RE_DATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\d{4}-\d{1,2}-\d{1,2}((T| )\d{2}:\d{2}:\d{2})?$").unwrap()
});
static RE_DATE_HAS_TIME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\d{4}-\d{1,2}-\d{1,2}((T| )\d{2}:\d{2}:\d{2})").unwrap()
});

/// Errors that can occur during query parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid JSON structure in IN clause")]
    InvalidInClause,
    #[error("Malformed query string")]
    MalformedQuery,
}

/**
  Transforms a WHERE sentence into a CAML Syntax sentence.
  Corresponds to the Javascript function `$SP().parse`.
*/
pub fn parse(q: &str, escape_char: bool) -> Result<String, ParseError> {
    // 1. Pre-processing
    let mut query_string = RE_OP_SPACE.replace_all(q, "$2").into_owned();
    query_string = RE_EMPTY_STR.replace_all(&query_string, "Null").into_owned();
    query_string = RE_EQ_EQ.replace_all(&query_string, "=").into_owned();

    // Handle Null inside IN clauses
    if RE_NULL_IN.is_match(&query_string) {
        query_string = RE_NULL_IN.replace_all(&query_string, "($1 = Null OR $0)").into_owned();
        query_string = RE_NULL_IN_CLEAN.replace_all(&query_string, "[$1$2]").into_owned();
        query_string = RE_NULL_IN_COMMAS.replace_all(&query_string, "$1$2$3").into_owned();
    }

    let mut factory: Vec<String> = Vec::new();
    let mut close_operator = String::new();
    let mut close_tag = String::new();
    let mut last_field = String::new();
    let mut lookup_id = false;

    let chars: Vec<char> = query_string.chars().collect();
    let limit_max = chars.len();
    let mut i = 0;

    while i < limit_max {
        let letter = chars[i];

        match letter {
            '(' => {
                let start = i;
                let mut open_count = 0;
                while i < limit_max && chars[i] == '(' {
                    i += 1;
                    open_count += 1;
                }

                let mut opened_apos = false;
                let mut ignore_next_char = false;
                while open_count > 0 && i < limit_max {
                    let char_at_i = chars[i];
                    if char_at_i == '\\' {
                        ignore_next_char = true;
                    } else if !ignore_next_char && (char_at_i == '\'' || char_at_i == '"') {
                        opened_apos = !opened_apos;
                    } else if !ignore_next_char && char_at_i == '(' && !opened_apos {
                        open_count += 1;
                    } else if !ignore_next_char && char_at_i == ')' && !opened_apos {
                        open_count -= 1;
                    } else {
                        ignore_next_char = false;
                    }
                    if open_count > 0 { i += 1; }
                }

                let sub_query = chars[start + 1..i].iter().collect::<String>();
                let parsed_sub = parse(&sub_query, escape_char)?;
                
                if !factory.is_empty() {
                    let mut combined = factory[0].clone();
                    if !close_operator.is_empty() {
                        combined = format!("<{}>{} {}</{}>", close_operator, combined, parsed_sub, close_operator);
                        close_operator.clear();
                    } else {
                        combined.push_str(&parsed_sub);
                    }
                    factory[0] = combined;
                } else {
                    factory.push(parsed_sub);
                }
            }
            '[' => {
                let start = i;
                let mut opened_apos = false;
                let mut ignore_next_char = false;
                while i < limit_max {
                    i += 1;
                    let char_at_i = chars[i];
                    if char_at_i == '\\' {
                        ignore_next_char = true;
                    } else if !ignore_next_char && (char_at_i == '\'' || char_at_i == '"') {
                        opened_apos = !opened_apos;
                    } else if !ignore_next_char && !opened_apos && char_at_i == ']' {
                        break;
                    } else {
                        ignore_next_char = false;
                    }
                }

                let array_str = format!("[{}]", chars[start + 1..i].iter().collect::<String>());
                let arr_in: Vec<JsonValue> = serde_json::from_str(&array_str).map_err(|_| ParseError::InvalidInClause)?;
                
                let mut type_in = "Text";
                let mut values_str = Vec::new();
                let mut is_lookup = false;

                if let Some(first) = arr_in.get(0) {
                    if first.is_number() {
                        type_in = "Number";
                    } else if let Some(s) = first.as_str() {
                        if s.starts_with('~') {
                            type_in = "Integer";
                            is_lookup = true;
                        }
                    }
                }

                for val in arr_in {
                    let mut s = match val {
                        JsonValue::String(s) => s,
                        JsonValue::Number(n) => n.to_string(),
                        _ => String::new(),
                    };
                    if is_lookup && s.starts_with('~') {
                        s = s[1..].to_string();
                    }
                    values_str.push(s);
                }

                let lookup_attr = if type_in == "Integer" { " LookupId=\"True\"" } else { "" };
                let caml_in = format!(
                    "<FieldRef Name=\"{}\"{} /><Values><Value Type=\"{}\">{}</Value></Values>",
                    last_field,
                    lookup_attr,
                    type_in,
                    values_str.join(&format!("</Value><Value Type=\"{}\">", type_in))
                );
                
                let last_idx = factory.len().saturating_sub(1);
                factory[last_idx].push_str(&caml_in);
                factory[last_idx].push_str(&close_tag);
                last_field.clear();
                close_tag.clear();

                if last_idx > 0 {
                    let term = factory.pop().unwrap();
                    if !close_operator.is_empty() {
                        factory[0] = format!("<{}>{} {}</{}>", close_operator, factory[0], term, close_operator);
                        close_operator.clear();
                    } else {
                        factory[0].push_str(&term);
                    }
                }
            }
            '>' | '<' => {
                i += 1;
                if i < limit_max && chars[i] == '=' {
                    let op = if letter == '>' { "Geq" } else { "Leq" };
                    factory.push(format!("<{}>", op));
                    close_tag = format!("</{}>", op);
                } else if letter == '<' && i < limit_max && chars[i] == '>' {
                    factory.push("<Neq>".to_string());
                    close_tag = "</Neq>".to_string();
                } else {
                    i -= 1;
                    let op = if letter == '>' { "Gt" } else { "Lt" };
                    factory.push(format!("<{}>", op));
                    close_tag = format!("</{}>", op);
                }
            }
            '~' => {
                if i + 1 < limit_max && (chars[i+1] == '=' || (chars[i+1] == '<' && i+2 < limit_max && chars[i+2] == '>')) {
                    lookup_id = true;
                }
            }
            '=' => {
                factory.push("<Eq>".to_string());
                close_tag = "</Eq>".to_string();
            }
            ' ' => {
                let remaining = chars[i..].iter().collect::<String>().to_uppercase();
                if remaining.starts_with(" AND ") {
                    close_operator = "And".to_string();
                    i += 4;
                } else if remaining.starts_with(" OR ") {
                    close_operator = "Or".to_string();
                    i += 3;
                } else if remaining.starts_with(" LIKE ") {
                    i += 5;
                    factory.push("<Contains>".to_string());
                    close_tag = "</Contains>".to_string();
                } else if remaining.starts_with(" IN ") {
                    i += 3;
                    factory.push("<In>".to_string());
                    close_tag = "</In>".to_string();
                } else {
                    last_field.push(letter);
                }
            }
            '"' | '\'' => {
                let apos = letter;
                let mut word = String::new();
                i += 1;
                while i < limit_max && chars[i] != apos {
                    if chars[i] == '\\' && i + 1 < limit_max && chars[i+1] == apos {
                        i += 1;
                    }
                    word.push(chars[i]);
                    i += 1;
                }

                let last_idx = factory.len().saturating_sub(1);
                let lookup_attr = if word == "[Me]" { " LookupId=\"True\" " } else { "" };
                factory[last_idx].push_str(&format!("<FieldRef Name=\"{}\"{}/>", last_field, lookup_attr));
                last_field.clear();

                let mut val_type = "Text";
                let mut other_attr = "";
                if RE_DATE.is_match(&word) {
                    val_type = "DateTime";
                    if RE_DATE_HAS_TIME.is_match(&word) {
                        other_attr = " IncludeTimeValue=\"TRUE\"";
                    }
                }

                let mut final_word = if escape_char { clean_string(Some(&word), None) } else { word };
                
                if final_word == "[Me]" {
                    final_word = "<UserID Type=\"Integer\" />".to_string();
                    val_type = "Integer";
                } else if final_word.starts_with("[Today") {
                    val_type = "DateTime";
                    let offset = final_word[6..final_word.len()-1].parse::<i32>().unwrap_or(0);
                    final_word = format!("<Today OffsetDays=\"{}\" />", offset);
                }

                factory[last_idx].push_str(&format!("<Value Type=\"{}\"{}>{}</Value>", val_type, other_attr, final_word));
                factory[last_idx].push_str(&close_tag);
                close_tag.clear();

                if last_idx > 0 {
                    let term = factory.pop().unwrap();
                    if !close_operator.is_empty() {
                        factory[0] = format!("<{}>{} {}</{}>", close_operator, factory[0], term, close_operator);
                        close_operator.clear();
                    } else {
                        factory[0].push_str(&term);
                    }
                }
            }
            '0'..='9' => {
                if !close_tag.is_empty() {
                    let mut value = letter.to_string();
                    i += 1;
                    while i < limit_max && chars[i].is_ascii_digit() {
                        value.push(chars[i]);
                        i += 1;
                    }
                    i -= 1; // Adjust for outer loop increment

                    let last_idx = factory.len().saturating_sub(1);
                    let lookup_attr = if lookup_id { " LookupId=\"True\"" } else { "" };
                    factory[last_idx].push_str(&format!("<FieldRef Name=\"{}\"{} />", last_field, lookup_attr));
                    last_field.clear();
                    
                    let val_type = if lookup_id { "Integer" } else { "Number" };
                    factory[last_idx].push_str(&format!("<Value Type=\"{}\">{}</Value>", val_type, value));
                    factory[last_idx].push_str(&close_tag);
                    close_tag.clear();
                    lookup_id = false;

                    if last_idx > 0 {
                        let term = factory.pop().unwrap();
                        if !close_operator.is_empty() {
                            factory[0] = format!("<{}>{} {}</{}>", close_operator, factory[0], term, close_operator);
                            close_operator.clear();
                        } else {
                            factory[0].push_str(&term);
                        }
                    }
                } else {
                    last_field.push(letter);
                }
            }
            _ => {
                if close_tag.is_empty() {
                    last_field.push(letter);
                } else {
                    let remaining = chars[i..].iter().collect::<String>().to_lowercase();
                    if remaining.starts_with("null") {
                        let last_idx = factory.len().saturating_sub(1);
                        if close_tag == "</Neq>" {
                            factory[last_idx] = "<IsNotNull>".to_string();
                            close_tag = "</IsNotNull>".to_string();
                        } else if close_tag == "</Eq>" {
                            factory[last_idx] = "<IsNull>".to_string();
                            close_tag = "</IsNull>".to_string();
                        }
                        i += 3;
                        factory[last_idx].push_str(&format!("<FieldRef Name=\"{}\" />", last_field));
                        last_field.clear();
                        factory[last_idx].push_str(&close_tag);
                        close_tag.clear();

                        if last_idx > 0 {
                            let term = factory.pop().unwrap();
                            if !close_operator.is_empty() {
                                factory[0] = format!("<{}>{} {}</{}>", close_operator, factory[0], term, close_operator);
                                close_operator.clear();
                            } else {
                                factory[0].push_str(&term);
                            }
                        }
                    } else if remaining.starts_with("true") || remaining.starts_with("false") {
                        let is_true = remaining.starts_with("true");
                        let last_idx = factory.len().saturating_sub(1);
                        i += if is_true { 3 } else { 4 };
                        
                        factory[last_idx].push_str(&format!(
                            "<FieldRef Name=\"{}\" /><Value Type=\"Boolean\">{}</Value>",
                            last_field,
                            if is_true { 1 } else { 0 }
                        ));
                        last_field.clear();
                        factory[last_idx].push_str(&close_tag);
                        close_tag.clear();

                        if last_idx > 0 {
                            let term = factory.pop().unwrap();
                            if !close_operator.is_empty() {
                                factory[0] = format!("<{}>{} {}</{}>", close_operator, factory[0], term, close_operator);
                                close_operator.clear();
                            } else {
                                factory[0].push_str(&term);
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    Ok(factory.join(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parse() {
        let query = "ContentType = \"My Content Type\" OR Description <> null";
        let result = parse(query, true).unwrap();
        assert!(result.contains("<Or>"));
        assert!(result.contains("<Eq><FieldRef Name=\"ContentType\" /><Value Type=\"Text\">My Content Type</Value></Eq>"));
        assert!(result.contains("<IsNotNull><FieldRef Name=\"Description\" /></IsNotNull>"));
    }
}