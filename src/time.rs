use chrono::{Local, NaiveDateTime, TimeZone};
use twilight_model::datetime::Timestamp;

#[inline]
pub fn seconds_to_string(seconds: i64) -> String {
    let datetime = NaiveDateTime::from_timestamp(seconds, 0);
    let datetime = Local.from_utc_datetime(&datetime);

    datetime.format("%H:%M:%S %d/%m/%Y").to_string()
}

#[inline]
pub fn timestamp_to_string(timestamp: Timestamp) -> String {
    seconds_to_string(timestamp.as_secs())
}
