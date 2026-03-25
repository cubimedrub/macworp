use http::HeaderValue;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContentRangeHeaderValueError {
    #[error(
        "Invalid format, missing {0} in {1} (format: `<unit:string> <range_start:uint>-<range_end:uint>/<size:uint>`)"
    )]
    InvalidFormat(&'static str, String),
    #[error(
        "Unable to parse {0} to {1} in {2} (format: `<unit:string> <range_start:uint>-<range_end:uint>/<size:uint>`)"
    )]
    Parsing(&'static str, &'static str, String),
}

pub struct ContentRangeHeaderValue {
    unit: String,
    range: (u64, u64),
    size: Option<u64>,
}

impl ContentRangeHeaderValue {
    pub fn new(unit: &str, range: (u64, u64), size: Option<u64>) -> Self {
        Self {
            range,
            size,
            unit: unit.to_string(),
        }
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub fn range(&self) -> (u64, u64) {
        self.range
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }
}

impl TryFrom<&HeaderValue> for ContentRangeHeaderValue {
    type Error = ContentRangeHeaderValueError;

    fn try_from(value: &HeaderValue) -> Result<Self, Self::Error> {
        let value_str = value.to_str().map_err(|_| {
            ContentRangeHeaderValueError::Parsing("header value", "string", format!("{value:?}"))
        })?;
        let mut parts = value_str.split_whitespace();
        let unit = parts
            .next()
            .ok_or(ContentRangeHeaderValueError::InvalidFormat(
                "unit",
                value_str.to_string(),
            ))?;
        let range_and_size = parts
            .next()
            .ok_or(ContentRangeHeaderValueError::InvalidFormat(
                "range and size",
                value_str.to_string(),
            ))?;
        let mut range_and_size_parts = range_and_size.split('/');
        let range_part =
            range_and_size_parts
                .next()
                .ok_or(ContentRangeHeaderValueError::InvalidFormat(
                    "range",
                    value_str.to_string(),
                ))?;
        let size_part =
            range_and_size_parts
                .next()
                .ok_or(ContentRangeHeaderValueError::InvalidFormat(
                    "size",
                    value_str.to_string(),
                ))?;

        let mut range_parts = range_part.split('-');
        let start = range_parts
            .next()
            .ok_or(ContentRangeHeaderValueError::InvalidFormat(
                "range start",
                value_str.to_string(),
            ))?
            .parse::<u64>()
            .map_err(|_| {
                ContentRangeHeaderValueError::Parsing("range start", "u64", value_str.to_string())
            })?;
        let end = range_parts
            .next()
            .ok_or(ContentRangeHeaderValueError::InvalidFormat(
                "range end",
                value_str.to_string(),
            ))?
            .parse::<u64>()
            .map_err(|_| {
                ContentRangeHeaderValueError::Parsing("range end", "u64", value_str.to_string())
            })?;

        let size = if size_part == "*" {
            None
        } else {
            Some(size_part.parse::<u64>().map_err(|_| {
                ContentRangeHeaderValueError::Parsing("size", "u64", value_str.to_string())
            })?)
        };

        Ok(Self::new(unit, (start, end), size))
    }
}

impl From<ContentRangeHeaderValue> for HeaderValue {
    fn from(value: ContentRangeHeaderValue) -> HeaderValue {
        let size_str = value
            .size
            .map(|size| size.to_string())
            .unwrap_or_else(|| "*".to_string());
        let value_str = format!(
            "{} {}-{}/{}",
            value.unit, value.range.0, value.range.1, size_str
        );
        HeaderValue::from_str(&value_str).unwrap()
    }
}
