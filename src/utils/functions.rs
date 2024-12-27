use chrono::{TimeZone, Utc};

pub fn number_days_since_2020() -> i64 {
    let today = Utc::now();
    let start_of_2020 = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    (today - start_of_2020).num_days()
}

pub fn anonymize_author<S: AsRef<str>>(author: S) -> String {
    // e.g. "John Doe" -> "JD"
    author
        .as_ref()
        .split_whitespace()
        .map(|word| word.chars().next().unwrap().to_uppercase().to_string())
        .collect()
}
