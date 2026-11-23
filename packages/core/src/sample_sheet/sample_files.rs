use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

/// Read number marker before the extension: `<prefix>_R1_001.fastq.gz`, `<prefix>_1.raw`,
/// `<prefix>.R2.fq`. The optional `_001` is an Illumina file part.
static READ_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?<prefix>.*?)[_.-]R?(?<read>[12])(?<tail>(?:_\d{3})?(?:\.[^./]+)+)$")
        .expect("valid pattern")
});

/// Illumina sample suffix (`_S1`, `_S1_L001`) dropped from names of paired reads.
static SAMPLE_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_S\d+(?:_L\d{3})?$").expect("valid pattern"));

/// Compression extensions that do not count as the file type.
const COMPRESSION: [&str; 6] = ["gz", "bz2", "xz", "zst", "zip", "tar"];

/// The files of one sample: any kind of raw or data file (sequencing reads, mass spectrometry
/// runs, images, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleFiles {
    /// Sample name derived from the file name
    pub sample: String,
    /// The file, or read 1 when files are paired
    pub first: String,
    /// Read 2, only when files were paired
    pub second: Option<String>,
}

impl SampleFiles {
    /// Turns the files of a folder into samples.
    ///
    /// Without `pair_reads` every file is one sample. With it, files that only differ in a read
    /// number marker (`R1`/`R2`, `_1`/`_2`) form one sample with two files; a lone read 2 stays
    /// a sample of its own so nothing is lost silently. Pairing is optional because numbered
    /// files are not always pairs (replicates of a mass spectrometry run, for example).
    ///
    /// # Arguments
    /// * `files` - project relative file paths (folders excluded)
    /// * `pattern` - only files whose path matches it are used, e.g. the pattern of the sheet's
    ///   file column; `None` takes every file
    /// * `pair_reads` - whether to pair read 1/read 2 files
    ///
    /// The result is sorted by path of the first file.
    pub fn group(files: &[String], pattern: Option<&Regex>, pair_reads: bool) -> Vec<SampleFiles> {
        let mut groups: BTreeMap<(String, String), [Option<&String>; 2]> = BTreeMap::new();
        let mut singles: Vec<&String> = Vec::new();
        for file in files
            .iter()
            .filter(|file| pattern.is_none_or(|p| p.is_match(file)))
        {
            let marker = pair_reads.then(|| READ_MARKER.captures(file)).flatten();
            match marker {
                Some(captures) => {
                    let key = (captures["prefix"].to_string(), captures["tail"].to_string());
                    let slot = usize::from(&captures["read"] == "2");
                    let entry = groups.entry(key).or_default();
                    if entry[slot].is_some() {
                        singles.push(file);
                    } else {
                        entry[slot] = Some(file);
                    }
                }
                None => singles.push(file),
            }
        }
        let mut samples: Vec<SampleFiles> = Vec::new();
        for ((prefix, _), [first, second]) in groups {
            match (first, second) {
                (Some(first), second) => samples.push(SampleFiles {
                    sample: SAMPLE_SUFFIX.replace(basename(&prefix), "").into_owned(),
                    first: first.clone(),
                    second: second.cloned(),
                }),
                (None, Some(only)) => singles.push(only),
                (None, None) => {}
            }
        }
        samples.extend(singles.into_iter().map(|file| SampleFiles {
            sample: strip_extension(basename(file)).to_string(),
            first: file.clone(),
            second: None,
        }));
        samples.sort_by(|a, b| a.first.cmp(&b.first));
        samples
    }
}

/// Last path component.
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// File name without trailing compression/archive extensions (`.tar.gz`) and one file type
/// extension.
fn strip_extension(name: &str) -> &str {
    let mut name = name;
    while let Some((stem, extension)) = name.rsplit_once('.')
        && COMPRESSION.contains(&extension.to_ascii_lowercase().as_str())
        && !stem.is_empty()
    {
        name = stem;
    }
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    /// Without pairing every file is a sample, whatever its type; numbered files stay apart.
    #[test]
    fn every_file_is_a_sample() {
        let samples = SampleFiles::group(
            &files(&[
                "raw/s_1.raw",
                "raw/s_2.raw",
                "raw/x.mzML.gz",
                "raw/plain",
                "raw/y.raw.tar.gz",
            ]),
            None,
            false,
        );
        let names: Vec<&str> = samples.iter().map(|s| s.sample.as_str()).collect();
        assert_eq!(names, ["plain", "s_1", "s_2", "x", "y"]);
        assert!(samples.iter().all(|s| s.second.is_none()));
    }

    /// Pairing groups read 1/2 of any extension and drops the Illumina suffix from the name.
    #[test]
    fn pairs_reads_when_asked() {
        let samples = SampleFiles::group(
            &files(&[
                "r/A_S1_L001_R2_001.fastq.gz",
                "r/A_S1_L001_R1_001.fastq.gz",
                "r/b_1.fq",
                "r/b_2.fq",
                "r/solo.fq",
                "r/c_2.fq",
            ]),
            None,
            true,
        );
        let summary: Vec<(&str, &str, Option<&str>)> = samples
            .iter()
            .map(|s| (s.sample.as_str(), s.first.as_str(), s.second.as_deref()))
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "A",
                    "r/A_S1_L001_R1_001.fastq.gz",
                    Some("r/A_S1_L001_R2_001.fastq.gz")
                ),
                ("b", "r/b_1.fq", Some("r/b_2.fq")),
                ("c_2", "r/c_2.fq", None),
                ("solo", "r/solo.fq", None),
            ]
        );
    }

    /// The same name in two folders stays two samples, and the pattern filters files.
    #[test]
    fn folders_and_pattern() {
        let pattern = Regex::new(r"\.raw$").unwrap();
        let samples = SampleFiles::group(
            &files(&["d1/s_1.raw", "d1/s_2.raw", "d2/s_1.raw", "notes.txt"]),
            Some(&pattern),
            true,
        );
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].second.as_deref(), Some("d1/s_2.raw"));
        assert_eq!(samples[1].second, None);
    }
}
