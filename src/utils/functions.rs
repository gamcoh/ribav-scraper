use chrono::{TimeZone, Utc};
use docx_rust::{
    document::Run,
    formatting::{CharacterProperty, CharacterStyleId},
};

pub fn number_days_since_2020() -> i64 {
    let today = Utc::now();
    let start_of_2020 = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    (today - start_of_2020).num_days()
}

pub fn anonymize_author<S: AsRef<str>>(author: S) -> String {
    if author.as_ref().to_lowercase().starts_with("rav ") {
        return author.as_ref().to_string();
    }

    // e.g. "John Doe" -> "JD"
    author
        .as_ref()
        .split_whitespace()
        .map(|word| word.chars().next().unwrap().to_uppercase().to_string())
        .collect()
}

pub fn is_citation(run: &Run) -> bool {
    run.property
        .as_ref()
        .unwrap_or(&CharacterProperty::default())
        .style_id
        .as_ref()
        .unwrap_or(&CharacterStyleId::from(""))
        .value
        .to_string()
        == "citation"
}
