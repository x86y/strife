use time::{format_description::FormatItem, macros::format_description, OffsetDateTime};
use twilight_model::util::Timestamp;

const FORMAT: &[FormatItem<'_>] =
    format_description!("[hour]:[minute]:[second] [day]/[month]/[year]");

pub fn display_timestamp(timestamp: Timestamp) -> String {
    OffsetDateTime::from_unix_timestamp(timestamp.as_secs())
        .unwrap()
        .format(FORMAT)
        .unwrap()
}
