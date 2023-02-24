use strife_discord::model::util::Timestamp;
use time::{format_description::FormatItem, macros::format_description, OffsetDateTime};

const FORMAT: &[FormatItem<'_>] =
    format_description!("[hour]:[minute]:[second] [day]/[month]/[year]");

pub fn display_timestamp(timestamp: Timestamp) -> String {
    OffsetDateTime::from_unix_timestamp(timestamp.as_secs())
        .unwrap()
        .format(FORMAT)
        .unwrap()
}
