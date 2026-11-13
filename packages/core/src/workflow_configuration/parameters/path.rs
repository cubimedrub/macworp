use std::{num::NonZeroUsize, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::workflow_configuration::parameters::parameter::{
    IsParameter, IsParameterWithValue, Parameter,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Path {
    #[serde(flatten)]
    inner: Parameter<Vec<PathBuf>>,
    /// Whether the parameter is exceptiung a file
    is_excepting_files: bool,
    /// Whether the parameter is excepting a folder
    is_excepting_folders: bool,
    /// Limit
    limit: NonZeroUsize,
    /// Optional regular expression a selected file's project-relative path (`/` separated) must
    /// match completely. `None` allows every file. Absent in older stored definitions.
    #[serde(default)]
    file_pattern: Option<String>,
    /// Optional regular expression a selected folder's project-relative path (`/` separated) must
    /// match completely. `None` allows every folder. Absent in older stored definitions.
    #[serde(default)]
    folder_pattern: Option<String>,
}

/// Longest accepted pattern, in bytes.
const MAX_PATTERN_LENGTH: usize = 512;

/// Compiled size limit of a pattern, in bytes. Bounds memory use of hostile patterns.
const PATTERN_SIZE_LIMIT: usize = 1 << 20;

/// Compiles `pattern` anchored at both ends, so it has to match the whole path.
///
/// # Arguments
/// * `pattern` - the user supplied regular expression.
///
/// Rejects patterns longer than [`MAX_PATTERN_LENGTH`], invalid syntax and patterns that compile
/// to more than [`PATTERN_SIZE_LIMIT`] bytes. The `regex` crate matches in linear time.
fn compile_pattern(pattern: &str) -> Result<regex::Regex, String> {
    if pattern.len() > MAX_PATTERN_LENGTH {
        return Err(format!("pattern longer than {MAX_PATTERN_LENGTH} bytes"));
    }
    regex::RegexBuilder::new(&format!("^(?:{pattern})$"))
        .size_limit(PATTERN_SIZE_LIMIT)
        .build()
        .map_err(|e| e.to_string())
}

/// Whether `path` matches the optional `pattern`.
///
/// # Arguments
/// * `pattern` - the pattern, `None` matches everything.
/// * `path` - project-relative path, matched with `/` separators.
///
/// An uncompilable pattern matches nothing.
fn matches_pattern(pattern: Option<&str>, path: &std::path::Path) -> bool {
    let Some(pattern) = pattern else {
        return true;
    };
    let text = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    compile_pattern(pattern).is_ok_and(|re| re.is_match(&text))
}

impl Path {
    pub fn new(name: String, label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(name, label, desc),
            is_excepting_files: false,
            is_excepting_folders: false,
            limit: NonZeroUsize::new(1).unwrap(),
            file_pattern: None,
            folder_pattern: None,
        }
    }

    /// Pattern selected files have to match, if any.
    pub fn file_pattern(&self) -> Option<&str> {
        self.file_pattern.as_deref()
    }

    /// Pattern selected folders have to match, if any.
    pub fn folder_pattern(&self) -> Option<&str> {
        self.folder_pattern.as_deref()
    }

    /// Sets the file pattern.
    ///
    /// # Arguments
    /// * `pattern` - new pattern; blank text clears it.
    pub fn set_file_pattern(&mut self, pattern: &str) {
        self.file_pattern = Some(pattern.to_string()).filter(|p| !p.trim().is_empty());
    }

    /// Sets the folder pattern.
    ///
    /// # Arguments
    /// * `pattern` - new pattern; blank text clears it.
    pub fn set_folder_pattern(&mut self, pattern: &str) {
        self.folder_pattern = Some(pattern.to_string()).filter(|p| !p.trim().is_empty());
    }

    /// Checks that both patterns are acceptable, for use when a definition is saved.
    ///
    /// Returns the message of the first rejected pattern, prefixed with its kind.
    pub fn validate_patterns(&self) -> Result<(), String> {
        for (kind, pattern) in [("file", &self.file_pattern), ("folder", &self.folder_pattern)] {
            if let Some(pattern) = pattern {
                compile_pattern(pattern).map_err(|e| format!("{kind} pattern: {e}"))?;
            }
        }
        Ok(())
    }

    /// Whether `path` may be selected as a file.
    ///
    /// # Arguments
    /// * `path` - project-relative path of the file.
    pub fn accepts_file(&self, path: &std::path::Path) -> bool {
        matches_pattern(self.file_pattern(), path)
    }

    /// Whether `path` may be selected as a folder.
    ///
    /// # Arguments
    /// * `path` - project-relative path of the folder.
    pub fn accepts_folder(&self, path: &std::path::Path) -> bool {
        matches_pattern(self.folder_pattern(), path)
    }

    /// Whether `path` is allowed by the patterns when its kind (file or folder) is unknown.
    ///
    /// # Arguments
    /// * `path` - project-relative path.
    ///
    /// Without any pattern everything is allowed. Otherwise the path has to satisfy the pattern of
    /// at least one kind the parameter accepts (both when neither kind is switched on).
    pub fn accepts_path(&self, path: &std::path::Path) -> bool {
        if self.file_pattern.is_none() && self.folder_pattern.is_none() {
            return true;
        }
        let neither = !self.is_excepting_files && !self.is_excepting_folders;
        ((self.is_excepting_files || neither) && self.accepts_file(path))
            || ((self.is_excepting_folders || neither) && self.accepts_folder(path))
    }

    pub fn is_excepting_files(&self) -> bool {
        self.is_excepting_files
    }

    pub fn is_excepting_files_mut(&mut self) -> &mut bool {
        &mut self.is_excepting_files
    }

    pub fn is_excepting_folders(&self) -> bool {
        self.is_excepting_folders
    }

    pub fn is_excepting_folders_mut(&mut self) -> &mut bool {
        &mut self.is_excepting_folders
    }

    pub fn limit(&self) -> NonZeroUsize {
        self.limit
    }

    pub fn limit_mut(&mut self) -> &mut NonZeroUsize {
        &mut self.limit
    }
    pub fn value_as_string(&self) -> String {
        self.inner
            .value()
            .map(|paths| {
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<String>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    pub fn value_from_string(&mut self, new_value: String) {
        if new_value.trim().is_empty() {
            *self.inner.value_mut() = None;
            return;
        }
        let paths: Vec<PathBuf> = new_value
            .split("\n")
            .map(|s| crate::paths::sanitize_relative(std::path::Path::new(s.trim())))
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        *self.inner.value_mut() = if paths.is_empty() { None } else { Some(paths) };
    }
}

impl IsParameter for Path {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn name_mut(&mut self) -> &mut String {
        self.inner.name_mut()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn label_mut(&mut self) -> &mut String {
        self.inner.label_mut()
    }

    fn desc(&self) -> &str {
        self.inner.desc()
    }

    fn desc_mut(&mut self) -> &mut String {
        self.inner.desc_mut()
    }
}

impl IsParameterWithValue<Vec<PathBuf>> for Path {
    fn value(&self) -> Option<&Vec<PathBuf>> {
        self.inner.value()
    }

    fn value_mut(&mut self) -> &mut Option<Vec<PathBuf>> {
        self.inner.value_mut()
    }
}

impl Default for Path {
    fn default() -> Self {
        Self {
            inner: Parameter::default(),
            is_excepting_files: true,
            is_excepting_folders: false,
            limit: NonZeroUsize::new(1).unwrap(),
            file_pattern: None,
            folder_pattern: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parameter with the given patterns, accepting files and folders.
    fn with(file: &str, folder: &str) -> Path {
        let mut p = Path::new("p".into(), "P".into(), String::new());
        *p.is_excepting_files_mut() = true;
        *p.is_excepting_folders_mut() = true;
        p.set_file_pattern(file);
        p.set_folder_pattern(folder);
        p
    }

    /// File and folder patterns are matched anchored against the relative path.
    #[test]
    fn matches_files_and_folders() {
        let p = with(r".*\.(fastq|fq)(\.gz)?", "data/.*");
        assert!(p.accepts_file("a/b.fq.gz".as_ref()));
        assert!(!p.accepts_file("a/b.fq.txt".as_ref()));
        assert!(!p.accepts_file("xa.fastqy".as_ref()));
        assert!(p.accepts_folder("data/x".as_ref()));
        assert!(!p.accepts_folder("other/x".as_ref()));
    }

    /// A partial match is not enough.
    #[test]
    fn pattern_is_anchored() {
        let p = with("a", "");
        assert!(p.accepts_file("a".as_ref()));
        assert!(!p.accepts_file("ba".as_ref()));
        assert!(!p.accepts_file("ab".as_ref()));
    }

    /// Invalid and oversized patterns are rejected on validation; blank clears.
    #[test]
    fn invalid_patterns_rejected() {
        assert!(with("(", "").validate_patterns().is_err());
        assert!(with("", "[").validate_patterns().is_err());
        assert!(with(&"a".repeat(513), "").validate_patterns().is_err());
        assert!(with("a{1000}{1000}{1000}", "").validate_patterns().is_err());
        assert!(with(r".*\.fq", "").validate_patterns().is_ok());
        assert_eq!(with(" ", "").file_pattern(), None);
    }

    /// Without patterns every path is accepted; patterns only apply to the kinds the parameter
    /// accepts.
    #[test]
    fn accepts_path_by_kind() {
        let mut p = Path::new("p".into(), "P".into(), String::new());
        assert!(p.accepts_path("anything".as_ref()));
        // Files only: the file pattern decides.
        *p.is_excepting_files_mut() = true;
        p.set_file_pattern(r".*\.fq");
        assert!(p.accepts_path("a.fq".as_ref()));
        assert!(!p.accepts_path("a.txt".as_ref()));
        // Folders are accepted too and have no pattern, so any path can be a folder.
        *p.is_excepting_folders_mut() = true;
        assert!(p.accepts_path("a.txt".as_ref()));
        // Folders only, with a folder pattern: the file pattern no longer counts.
        *p.is_excepting_files_mut() = false;
        p.set_folder_pattern("data");
        assert!(!p.accepts_path("a.fq".as_ref()));
        assert!(p.accepts_path("data".as_ref()));
    }

    /// Definitions stored before the patterns existed still load.
    #[test]
    fn old_definition_deserializes() {
        let json = serde_json::to_value(Path::default()).unwrap();
        let mut object = json.as_object().unwrap().clone();
        object.remove("file_pattern");
        object.remove("folder_pattern");
        let p: Path = serde_json::from_value(object.into()).unwrap();
        assert_eq!(p.file_pattern(), None);
    }
}
