use std::collections::HashSet;

use crate::schema_validation::{PathChecker, PathKind};

use super::{Column, ColumnKind, SampleFiles, Sheet};

impl Sheet {
    /// A sheet with the schema's columns and no rows.
    ///
    /// # Arguments
    /// * `columns` - columns derived from the schema
    pub fn with_columns(columns: &[Column]) -> Sheet {
        Sheet {
            columns: columns.iter().map(|column| column.name.clone()).collect(),
            rows: Vec::new(),
        }
    }

    /// Appends a row holding the schema defaults (empty where there is none).
    ///
    /// # Arguments
    /// * `columns` - columns derived from the schema, for the defaults
    pub fn add_row(&mut self, columns: &[Column]) {
        let row = self
            .columns
            .iter()
            .map(|name| {
                columns
                    .iter()
                    .find(|column| &column.name == name)
                    .and_then(|column| column.default.clone())
                    .unwrap_or_default()
            })
            .collect();
        self.rows.push(row);
    }

    /// Removes a row; out of range is ignored.
    ///
    /// # Arguments
    /// * `row` - zero-based row
    pub fn remove_row(&mut self, row: usize) {
        if row < self.rows.len() {
            self.rows.remove(row);
        }
    }

    /// Copies a row below itself; out of range is ignored.
    ///
    /// # Arguments
    /// * `row` - zero-based row
    pub fn duplicate_row(&mut self, row: usize) {
        if let Some(copy) = self.rows.get(row).cloned() {
            self.rows.insert(row + 1, copy);
        }
    }

    /// Sets a cell; out of range is ignored.
    ///
    /// # Arguments
    /// * `row` - zero-based row
    /// * `column` - zero-based column
    /// * `value` - new text
    pub fn set_cell(&mut self, row: usize, column: usize, value: String) {
        if let Some(cell) = self
            .rows
            .get_mut(row)
            .and_then(|cells| cells.get_mut(column))
        {
            *cell = value;
        }
    }

    /// Adds the schema columns the sheet lacks (existing rows get empty cells), so a loaded
    /// sheet can be edited in every column the schema knows. Extra columns stay.
    ///
    /// # Arguments
    /// * `columns` - columns derived from the schema
    pub fn ensure_columns(&mut self, columns: &[Column]) {
        for column in columns {
            if !self.columns.contains(&column.name) {
                self.columns.push(column.name.clone());
                for row in &mut self.rows {
                    row.push(String::new());
                }
            }
        }
    }

    /// Appends one row per sample. The sample name goes to the column called `sample` (else the
    /// first text column), the first file to the first file or path column and the second file (paired
    /// reads) to the second one (columns of kind file or path); other columns get their default.
    ///
    /// # Arguments
    /// * `samples` - result of [`SampleFiles::group`]
    /// * `columns` - columns derived from the schema
    pub fn append_samples(&mut self, samples: &[SampleFiles], columns: &[Column]) {
        let is_file = |name: &String| {
            columns.iter().any(|column| {
                &column.name == name && matches!(column.kind, ColumnKind::File | ColumnKind::Path)
            })
        };
        let file_columns: Vec<usize> = (0..self.columns.len())
            .filter(|&index| is_file(&self.columns[index]))
            .collect();
        let sample_column = self
            .columns
            .iter()
            .position(|name| name == "sample")
            .or_else(|| {
                (0..self.columns.len()).find(|&index| {
                    columns.iter().any(|column| {
                        column.name == self.columns[index]
                            && matches!(column.kind, ColumnKind::Value(_))
                    })
                })
            });
        for sample in samples {
            self.add_row(columns);
            let row = self.rows.len() - 1;
            if let Some(index) = sample_column {
                self.rows[row][index] = sample.sample.clone();
            }
            if let Some(&index) = file_columns.first() {
                self.rows[row][index] = sample.first.clone();
            }
            if let (Some(&index), Some(second)) = (file_columns.get(1), &sample.second) {
                self.rows[row][index] = second.clone();
            }
        }
    }

    /// Folders that contain the paths of file/folder/path cells, to list them before checking
    /// whether the paths exist. The project root is the empty string.
    ///
    /// # Arguments
    /// * `columns` - columns derived from the schema
    pub fn referenced_directories(&self, columns: &[Column]) -> Vec<String> {
        let mut directories = HashSet::new();
        for (index, name) in self.columns.iter().enumerate() {
            let is_path = columns.iter().any(|column| {
                &column.name == name
                    && matches!(
                        column.kind,
                        ColumnKind::File | ColumnKind::Directory | ColumnKind::Path
                    )
            });
            if !is_path {
                continue;
            }
            for row in &self.rows {
                let value = row.get(index).map(String::as_str).unwrap_or_default();
                if !value.is_empty() {
                    directories.insert(parent_of(value).to_string());
                }
            }
        }
        let mut directories: Vec<String> = directories.into_iter().collect();
        directories.sort();
        directories
    }
}

/// Folder part of a project relative path (empty for the root).
fn parent_of(path: &str) -> &str {
    path.trim_start_matches("./")
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

/// A [`PathChecker`] over folder listings, for the browser: list the folders named by
/// [`Sheet::referenced_directories`], add them, then validate. A path in a folder that was not
/// added counts as missing.
#[derive(Debug, Clone, Default)]
pub struct ListingChecker {
    /// Project relative paths of files
    files: HashSet<String>,
    /// Project relative paths of folders
    directories: HashSet<String>,
}

impl ListingChecker {
    /// Records the content of one folder.
    ///
    /// # Arguments
    /// * `directory` - project relative folder path (empty for the root)
    /// * `files` - file names in it
    /// * `directories` - folder names in it
    pub fn add_listing(&mut self, directory: &str, files: &[String], directories: &[String]) {
        let join = |name: &String| {
            if directory.is_empty() {
                name.clone()
            } else {
                format!("{directory}/{name}")
            }
        };
        self.files.extend(files.iter().map(join));
        self.directories.extend(directories.iter().map(join));
    }
}

impl PathChecker for ListingChecker {
    fn exists(&self, path: &str, kind: PathKind) -> bool {
        let path = path.trim_start_matches("./");
        match kind {
            PathKind::File => self.files.contains(path),
            PathKind::Directory => self.directories.contains(path),
            PathKind::Any => self.files.contains(path) || self.directories.contains(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample_sheet::Format;

    fn demo_columns() -> Vec<Column> {
        let schema: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/nf_core_demo_schema_input.json"
        ))
        .unwrap();
        Column::from_schema(&schema)
    }

    /// New sheets, row operations, bounds.
    #[test]
    fn row_operations() {
        let columns = demo_columns();
        let mut sheet = Sheet::with_columns(&columns);
        assert_eq!(sheet.columns, ["sample", "fastq_1", "fastq_2"]);
        sheet.add_row(&columns);
        sheet.set_cell(0, 0, "A".into());
        sheet.duplicate_row(0);
        sheet.set_cell(1, 0, "B".into());
        sheet.set_cell(9, 9, "ignored".into());
        assert_eq!(sheet.rows[0][0], "A");
        assert_eq!(sheet.rows[1][0], "B");
        sheet.remove_row(0);
        sheet.remove_row(5);
        assert_eq!(sheet.rows.len(), 1);
    }

    /// Missing schema columns are appended with empty cells; extra ones stay.
    #[test]
    fn ensure_columns_pads_rows() {
        let mut sheet = Sheet::parse(Format::Csv, "sample,extra\nA,x\n").unwrap();
        sheet.ensure_columns(&demo_columns());
        assert_eq!(sheet.columns, ["sample", "extra", "fastq_1", "fastq_2"]);
        assert_eq!(sheet.rows[0], ["A", "x", "", ""]);
    }

    /// Sample files become rows: sample name, first and second file columns.
    #[test]
    fn samples_become_rows() {
        let columns = demo_columns();
        let mut sheet = Sheet::with_columns(&columns);
        let samples = SampleFiles::group(
            &[
                "r/a_1.fq.gz".into(),
                "r/a_2.fq.gz".into(),
                "r/b.fq.gz".into(),
            ],
            None,
            true,
        );
        sheet.append_samples(&samples, &columns);
        assert_eq!(sheet.rows[0], ["a", "r/a_1.fq.gz", "r/a_2.fq.gz"]);
        assert_eq!(sheet.rows[1], ["b", "r/b.fq.gz", ""]);
    }

    /// Columns of kind `path` (file or folder) receive samples like file columns.
    #[test]
    fn path_columns_receive_files() {
        let schema = serde_json::json!({"items": {"properties": {
            "sample": {"type": "string"},
            "spectrum_file": {"type": "string", "format": "path"}
        }}});
        let columns = Column::from_schema(&schema);
        let mut sheet = Sheet::with_columns(&columns);
        let samples = SampleFiles::group(&["raw/a.raw".into(), "raw/b.d".into()], None, false);
        sheet.append_samples(&samples, &columns);
        assert_eq!(sheet.rows[0], ["a", "raw/a.raw"]);
        assert_eq!(sheet.rows[1], ["b", "raw/b.d"]);
    }

    /// Existence check from listings: folders to list, then files found or missing.
    #[test]
    fn listing_checker_flags_missing_files() {
        let columns = demo_columns();
        let sheet = Sheet::parse(
            Format::Csv,
            "sample,fastq_1\nA,reads/a.fastq.gz\nB,b.fastq.gz\nC,\n",
        )
        .unwrap();
        assert_eq!(sheet.referenced_directories(&columns), ["", "reads"]);
        let schema: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/nf_core_demo_schema_input.json"
        ))
        .unwrap();
        let validator = crate::schema_validation::SchemaValidator::new(&schema).unwrap();
        let mut checker = ListingChecker::default();
        checker.add_listing("reads", &["a.fastq.gz".into()], &[]);
        checker.add_listing("", &[], &["reads".into()]);
        let issues = sheet.validate(&validator, &columns, &checker);
        assert!(
            issues
                .iter()
                .any(|i| i.row == Some(1) && i.keyword == "exists"),
            "{issues:?}"
        );
        assert!(
            !issues
                .iter()
                .any(|i| i.row == Some(0) && i.keyword == "exists"),
            "{issues:?}"
        );
    }
}
