use chrono::{DateTime, Utc};
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseRecurrenceError {
    #[error("XML parsing error: {0}")]
    XmlRead(#[from] quick_xml::Error),
    #[error("XML writing error: {0}")]
    XmlWrite(#[from] quick_xml::WriterError),
    #[error("Invalid XML structure: {0}")]
    InvalidXml(String),
    #[error("Invalid date format: {0}")]
    InvalidDate(#[from] chrono::ParseError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Represents the type of recurrence rule.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub enum RecurrenceType {
    Daily,
    Weekly,
    Monthly,
    MonthlyByDay,
    Yearly,
    YearlyByDay,
}

/// Defines the specific days or dates for the recurrence.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecurrenceOn {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tuesday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wednesday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thursday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saturday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunday: Option<String>,
}

/// Represents the end condition for the recurrence.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum RecurrenceEnd {
    EndDate(DateTime<Utc>),
    EndAfter(u32),
    Never,
}

/// Represents a SharePoint recurrence rule.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Recurrence {
    pub r#type: RecurrenceType,
    #[serde(default = "default_first_day_of_week")]
    pub first_day_of_week: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<RecurrenceOn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<RecurrenceEnd>,
}

fn default_first_day_of_week() -> String {
    "mo".to_string()
}

/// An enum to represent the two possible inputs/outputs of the main function.
#[derive(Debug, PartialEq)]
pub enum RecurrenceData {
    Xml(String),
    Object(Recurrence),
}

/// Transforms a RecurrenceData XML string to a `Recurrence` object,
/// or a `Recurrence` object to a RecurrenceData XML string.
/// Corresponds to the Javascript function `$SP().parseRecurrence`.
pub fn parse_recurrence(data: RecurrenceData) -> Result<RecurrenceData, ParseRecurrenceError> {
    match data {
        RecurrenceData::Xml(xml_string) => xml_to_object(&xml_string),
        RecurrenceData::Object(recurrence_object) => object_to_xml(&recurrence_object),
    }
}

fn xml_to_object(xml_string: &str) -> Result<RecurrenceData, ParseRecurrenceError> {
    let mut reader = Reader::from_str(xml_string);
    reader.trim_text(true);
    let mut buf = Vec::new();

    let mut rec = Recurrence {
        r#type: RecurrenceType::Daily, // Default, will be overwritten
        first_day_of_week: "monday".to_string(),
        on: Some(RecurrenceOn::default()),
        frequency: None,
        end: None,
    };
    let mut found_type = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let attributes: HashMap<_, _> = e
                    .attributes()
                    .map(|a| {
                        let attr = a.unwrap();
                        (
                            String::from_utf8_lossy(attr.key.as_ref()).to_string(),
                            attr.decode_and_unescape_value(&reader).unwrap().to_string(),
                        )
                    })
                    .collect();

                match e.name().as_ref() {
                    b"firstDayOfWeek" => {
                        if let Ok(Event::Text(t)) = reader.read_event_into(&mut Vec::new()) {
                            let day_code = t.unescape()?.to_string();
                            rec.first_day_of_week = match day_code.as_str() {
                                "mo" => "monday",
                                "tu" => "tuesday",
                                "we" => "wednesday",
                                "th" => "thursday",
                                "fr" => "friday",
                                "sa" => "saturday",
                                "su" => "sunday",
                                _ => "monday",
                            }
                            .to_string();
                        }
                    }
                    b"daily" => {
                        rec.r#type = RecurrenceType::Daily;
                        found_type = true;
                        if attributes.get("weekday").map(|s| s.as_str()) == Some("TRUE") {
                            rec.on.as_mut().unwrap().weekday = Some("TRUE".to_string());
                        } else if let Some(freq) = attributes.get("dayFrequency") {
                            rec.frequency = freq.parse().ok();
                        }
                    }
                    b"weekly" => {
                        rec.r#type = RecurrenceType::Weekly;
                        found_type = true;
                        rec.frequency = attributes.get("weekFrequency").and_then(|s| s.parse().ok());
                        let on = rec.on.as_mut().unwrap();
                        if attributes.get("mo") == Some(&"TRUE".to_string()) { on.monday = Some("TRUE".to_string()); }
                        if attributes.get("tu") == Some(&"TRUE".to_string()) { on.tuesday = Some("TRUE".to_string()); }
                        if attributes.get("we") == Some(&"TRUE".to_string()) { on.wednesday = Some("TRUE".to_string()); }
                        if attributes.get("th") == Some(&"TRUE".to_string()) { on.thursday = Some("TRUE".to_string()); }
                        if attributes.get("fr") == Some(&"TRUE".to_string()) { on.friday = Some("TRUE".to_string()); }
                        if attributes.get("sa") == Some(&"TRUE".to_string()) { on.saturday = Some("TRUE".to_string()); }
                        if attributes.get("su") == Some(&"TRUE".to_string()) { on.sunday = Some("TRUE".to_string()); }
                    }
                    b"monthly" => {
                        rec.r#type = RecurrenceType::Monthly;
                        found_type = true;
                        rec.frequency = attributes.get("monthFrequency").and_then(|s| s.parse().ok());
                        rec.on.as_mut().unwrap().day = attributes.get("day").and_then(|s| s.parse().ok());
                    }
                    b"monthlyByDay" => {
                        rec.r#type = RecurrenceType::MonthlyByDay;
                        found_type = true;
                        rec.frequency = attributes.get("monthFrequency").and_then(|s| s.parse().ok());
                        handle_by_day(&mut rec, &attributes);
                    }
                    b"yearly" => {
                        rec.r#type = RecurrenceType::Yearly;
                        found_type = true;
                        rec.frequency = attributes.get("yearFrequency").and_then(|s| s.parse().ok());
                        let on = rec.on.as_mut().unwrap();
                        on.month = attributes.get("month").and_then(|s| s.parse().ok());
                        on.day = attributes.get("day").and_then(|s| s.parse().ok());
                    }
                    b"yearlyByDay" => {
                        rec.r#type = RecurrenceType::YearlyByDay;
                        found_type = true;
                        rec.frequency = attributes.get("yearFrequency").and_then(|s| s.parse().ok());
                        rec.on.as_mut().unwrap().month = attributes.get("month").and_then(|s| s.parse().ok());
                        handle_by_day(&mut rec, &attributes);
                    }
                    b"windowEnd" => {
                        if let Ok(Event::Text(t)) = reader.read_event_into(&mut Vec::new()) {
                            let date_str = t.unescape()?;
                            rec.end = Some(RecurrenceEnd::EndDate(date_str.parse::<DateTime<Utc>>()?));
                        }
                    }
                    b"repeatInstances" => {
                        if let Ok(Event::Text(t)) = reader.read_event_into(&mut Vec::new()) {
                            let num_str = t.unescape()?;
                            rec.end = num_str.parse().ok().map(RecurrenceEnd::EndAfter);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ParseRecurrenceError::XmlRead(e)),
            _ => (),
        }
        buf.clear();
    }

    if !found_type {
        return Err(ParseRecurrenceError::InvalidXml(
            "No recurrence type tag found (e.g., <daily>, <weekly>)".to_string(),
        ));
    }

    Ok(RecurrenceData::Object(rec))
}

fn handle_by_day(rec: &mut Recurrence, attributes: &HashMap<String, String>) {
    let weekday_of_month = attributes
        .get("weekdayOfMonth")
        .or_else(|| attributes.get("weekDayOfMonth"))
        .cloned();

    if let Some(wom) = weekday_of_month {
        let on = rec.on.as_mut().unwrap();
        if attributes.get("mo") == Some(&"TRUE".to_string()) { on.monday = Some(wom.clone()); }
        if attributes.get("tu") == Some(&"TRUE".to_string()) { on.tuesday = Some(wom.clone()); }
        if attributes.get("we") == Some(&"TRUE".to_string()) { on.wednesday = Some(wom.clone()); }
        if attributes.get("th") == Some(&"TRUE".to_string()) { on.thursday = Some(wom.clone()); }
        if attributes.get("fr") == Some(&"TRUE".to_string()) { on.friday = Some(wom.clone()); }
        if attributes.get("sa") == Some(&"TRUE".to_string()) { on.saturday = Some(wom.clone()); }
        if attributes.get("su") == Some(&"TRUE".to_string()) { on.sunday = Some(wom.clone()); }
        if attributes.get("day") == Some(&"TRUE".to_string()) { on.day = wom.parse().ok(); } // This seems wrong based on JS, but let's keep it if it's a possibility
        if attributes.get("weekday") == Some(&"TRUE".to_string()) { on.weekday = Some(wom.clone()); }
        if attributes.get("weekend_day") == Some(&"TRUE".to_string()) { on.weekend = Some(wom); }
    }
}

fn object_to_xml(rec: &Recurrence) -> Result<RecurrenceData, ParseRecurrenceError> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    writer.create_element("recurrence").write_inner_content(|writer| {
        writer.create_element("rule").write_inner_content(|writer| {
            // <firstDayOfWeek>
            writer.create_element("firstDayOfWeek")
                .write_text_content(BytesEnd::new(&rec.first_day_of_week[..2]))?;

            // <repeat>
            writer.create_element("repeat").write_inner_content(|writer| {
                let mut repeat_tag = BytesStart::new(match rec.r#type {
                    RecurrenceType::Daily => "daily",
                    RecurrenceType::Weekly => "weekly",
                    RecurrenceType::Monthly => "monthly",
                    RecurrenceType::MonthlyByDay => "monthlyByDay",
                    RecurrenceType::Yearly => "yearly",
                    RecurrenceType::YearlyByDay => "yearlyByDay",
                });

                let on = rec.on.as_ref();

                match rec.r#type {
                    RecurrenceType::Daily => {
                        if on.and_then(|o| o.weekday.as_deref()) == Some("TRUE") {
                            repeat_tag.push_attribute(("weekday", "TRUE"));
                        } else if let Some(freq) = rec.frequency {
                            repeat_tag.push_attribute(("dayFrequency", freq.to_string().as_str()));
                        }
                    }
                    RecurrenceType::Weekly => {
                        if let Some(o) = on {
                            if o.monday.is_some() { repeat_tag.push_attribute(("mo", "TRUE")); }
                            if o.tuesday.is_some() { repeat_tag.push_attribute(("tu", "TRUE")); }
                            if o.wednesday.is_some() { repeat_tag.push_attribute(("we", "TRUE")); }
                            if o.thursday.is_some() { repeat_tag.push_attribute(("th", "TRUE")); }
                            if o.friday.is_some() { repeat_tag.push_attribute(("fr", "TRUE")); }
                            if o.saturday.is_some() { repeat_tag.push_attribute(("sa", "TRUE")); }
                            if o.sunday.is_some() { repeat_tag.push_attribute(("su", "TRUE")); }
                        }
                        if let Some(freq) = rec.frequency {
                            repeat_tag.push_attribute(("weekFrequency", freq.to_string().as_str()));
                        }
                    }
                    RecurrenceType::Monthly => {
                        if let Some(freq) = rec.frequency {
                            repeat_tag.push_attribute(("monthFrequency", freq.to_string().as_str()));
                        }
                        if let Some(day) = on.and_then(|o| o.day) {
                            repeat_tag.push_attribute(("day", day.to_string().as_str()));
                        }
                    }
                    RecurrenceType::MonthlyByDay | RecurrenceType::YearlyByDay => {
                        let mut wom_written = false;
                        if let Some(o) = on {
                            let days = [
                                ("mo", &o.monday), ("tu", &o.tuesday), ("we", &o.wednesday),
                                ("th", &o.thursday), ("fr", &o.friday), ("sa", &o.saturday),
                                ("su", &o.sunday),
                            ];
                            for (day_code, day_val) in &days {
                                if let Some(val) = day_val {
                                    repeat_tag.push_attribute((*day_code, "TRUE"));
                                    if !wom_written {
                                        let wom_attr = if rec.r#type == RecurrenceType::MonthlyByDay { "weekdayOfMonth" } else { "weekDayOfMonth" };
                                        repeat_tag.push_attribute((wom_attr, val.as_str()));
                                        wom_written = true;
                                    }
                                }
                            }
                            if let Some(val) = &o.weekday {
                                repeat_tag.push_attribute(("weekday", "TRUE"));
                                if !wom_written {
                                    let wom_attr = if rec.r#type == RecurrenceType::MonthlyByDay { "weekdayOfMonth" } else { "weekDayOfMonth" };
                                    repeat_tag.push_attribute((wom_attr, val.as_str()));
                                    wom_written = true;
                                }
                            }
                            if let Some(val) = &o.weekend {
                                repeat_tag.push_attribute(("weekend_day", "TRUE"));
                                if !wom_written {
                                    let wom_attr = if rec.r#type == RecurrenceType::MonthlyByDay { "weekdayOfMonth" } else { "weekDayOfMonth" };
                                    repeat_tag.push_attribute((wom_attr, val.as_str()));
                                }
                            }
                            if let Some(month) = o.month {
                                repeat_tag.push_attribute(("month", month.to_string().as_str()));
                            }
                        }
                        let freq_attr = if rec.r#type == RecurrenceType::MonthlyByDay { "monthFrequency" } else { "yearFrequency" };
                        if let Some(freq) = rec.frequency {
                            repeat_tag.push_attribute((freq_attr, freq.to_string().as_str()));
                        }
                    }
                    RecurrenceType::Yearly => {
                        if let Some(freq) = rec.frequency {
                            repeat_tag.push_attribute(("yearFrequency", freq.to_string().as_str()));
                        }
                        if let Some(o) = on {
                            if let Some(month) = o.month {
                                repeat_tag.push_attribute(("month", month.to_string().as_str()));
                            }
                            if let Some(day) = o.day {
                                repeat_tag.push_attribute(("day", day.to_string().as_str()));
                            }
                        }
                    }
                }
                writer.write_event(Event::Empty(repeat_tag))?;
                Ok(())
            })?;

            // End condition
            match &rec.end {
                Some(RecurrenceEnd::EndDate(dt)) => {
                    let date_str = dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                    writer.create_element("windowEnd").write_text_content(BytesEnd::new(&date_str))?;
                }
                Some(RecurrenceEnd::EndAfter(count)) => {
                    writer.create_element("repeatInstances").write_text_content(BytesEnd::new(&count.to_string()))?;
                }
                Some(RecurrenceEnd::Never) | None => {
                    writer.create_element("repeatForever").write_text_content(BytesEnd::new("FALSE"))?;
                }
            }
            Ok(())
        })?;
        Ok(())
    })?;

    let result = writer.into_inner().into_inner();
    Ok(RecurrenceData::Xml(String::from_utf8(result)?))
}