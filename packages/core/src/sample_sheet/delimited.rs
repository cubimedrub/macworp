//! Delimiter separated text (RFC 4180 style): quoted fields, doubled quotes, embedded line
//! breaks, CRLF and LF line ends, optional byte order mark.

/// Splits delimiter separated text into records of fields.
///
/// A trailing line break does not create an empty record. Completely empty lines are skipped.
///
/// # Arguments
/// * `text` - the file content
/// * `delimiter` - field separator (`,` or `\t`)
pub(super) fn parse_records(text: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut field_started = false;
    let mut line = 1usize;
    let mut chars = text.chars().peekable();

    while let Some(character) = chars.next() {
        if quoted {
            match character {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                other => {
                    if other == '\n' {
                        line += 1;
                    }
                    field.push(other);
                }
            }
            continue;
        }
        match character {
            '"' if !field_started => {
                quoted = true;
                field_started = true;
            }
            '"' => return Err(format!("unexpected quote in line {line}")),
            character if character == delimiter => {
                record.push(std::mem::take(&mut field));
                field_started = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                line += 1;
                if field_started || !record.is_empty() || !field.is_empty() {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                }
                field_started = false;
            }
            other => {
                field.push(other);
                field_started = true;
            }
        }
    }
    if quoted {
        return Err("unterminated quoted field".to_string());
    }
    if field_started || !record.is_empty() || !field.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

/// Writes records as delimiter separated text with LF line ends. Fields are quoted only when
/// they contain the delimiter, a quote or a line break.
///
/// # Arguments
/// * `records` - rows of fields (the header is the first record)
/// * `delimiter` - field separator (`,` or `\t`)
pub(super) fn write_records(records: &[Vec<String>], delimiter: char) -> String {
    let mut output = String::new();
    for record in records {
        for (index, field) in record.iter().enumerate() {
            if index > 0 {
                output.push(delimiter);
            }
            if field.contains([delimiter, '"', '\n', '\r']) {
                output.push('"');
                output.push_str(&field.replace('"', "\"\""));
                output.push('"');
            } else {
                output.push_str(field);
            }
        }
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(fields: &[&str]) -> Vec<String> {
        fields.iter().map(|f| f.to_string()).collect()
    }

    /// Plain, quoted, escaped quote and embedded line break fields.
    #[test]
    fn parses_quoting() {
        let text = "a,b\n\"x,y\",\"say \"\"hi\"\"\"\n\"two\nlines\",z\n";
        assert_eq!(
            parse_records(text, ',').unwrap(),
            vec![
                rec(&["a", "b"]),
                rec(&["x,y", "say \"hi\""]),
                rec(&["two\nlines", "z"])
            ]
        );
    }

    /// BOM, CRLF, missing final newline, blank lines and empty fields.
    #[test]
    fn tolerates_odd_files() {
        let text = "\u{feff}a,b\r\n\r\n1,\r\n,2";
        assert_eq!(
            parse_records(text, ',').unwrap(),
            vec![rec(&["a", "b"]), rec(&["1", ""]), rec(&["", "2"])]
        );
        assert!(parse_records("", ',').unwrap().is_empty());
    }

    /// Broken quoting is reported.
    #[test]
    fn rejects_broken_quotes() {
        assert!(parse_records("a,\"b\n", ',').is_err());
        assert!(parse_records("a,b\"c\n", ',').is_err());
    }

    /// Writing then parsing gives the input back, also with tabs as delimiter.
    #[test]
    fn round_trip() {
        let records = vec![
            rec(&["a", "b\tc"]),
            rec(&["x,y", "q\"r"]),
            rec(&["", "l\nm"]),
        ];
        for delimiter in [',', '\t'] {
            let text = write_records(&records, delimiter);
            assert_eq!(parse_records(&text, delimiter).unwrap(), records);
        }
    }
}
