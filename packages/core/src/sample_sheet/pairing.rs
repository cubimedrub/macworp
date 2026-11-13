use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

/// Read number marker at the end of a FASTQ name: `<prefix>_R1_001.fastq.gz`, `<prefix>_1.fq`,
/// `<prefix>.R2.fastq.gz`, ... The optional `_001` chunk is the Illumina file part.
static READ_FILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?<prefix>.*?)[_.-]R?(?<read>[12])(?<tail>(?:_\d{3})?\.(?:fastq|fq)(?:\.gz)?)$")
        .expect("valid pattern")
});

/// Illumina sample suffix (`_S1`, `_S1_L001`) dropped from guessed sample names.
static SAMPLE_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_S\d+(?:_L\d{3})?$").expect("valid pattern"));

/// The read files of one sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPair {
    /// Sample name guessed from the file name
    pub sample: String,
    /// Forward reads (read 1)
    pub forward: String,
    /// Reverse reads (read 2), `None` for single-end data
    pub reverse: Option<String>,
}

impl ReadPair {
    /// Groups FASTQ files into samples: files that only differ in the read number (`R1`/`R2`,
    /// `_1`/`_2`) form a pair, every other FASTQ file is a single-end sample. Files that are
    /// not FASTQ are ignored, a lone read 2 file is kept as single-end so nothing is lost
    /// silently. Result is sorted by file path.
    ///
    /// # Arguments
    /// * `files` - project relative file paths (folders excluded)
    pub fn pair(files: &[String]) -> Vec<ReadPair> {
        let mut groups: BTreeMap<(String, String), [Option<&String>; 2]> = BTreeMap::new();
        let mut singles: Vec<&String> = Vec::new();
        for file in files {
            match READ_FILE.captures(file) {
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
                None if is_fastq(file) => singles.push(file),
                None => {}
            }
        }
        let mut pairs: Vec<ReadPair> = Vec::new();
        for ((prefix, _), [forward, reverse]) in groups {
            match (forward, reverse) {
                (Some(forward), reverse) => pairs.push(ReadPair {
                    sample: sample_name(&prefix),
                    forward: forward.clone(),
                    reverse: reverse.cloned(),
                }),
                (None, Some(only)) => singles.push(only),
                (None, None) => {}
            }
        }
        pairs.extend(singles.into_iter().map(|file| ReadPair {
            sample: sample_name(strip_extension(basename(file))),
            forward: file.clone(),
            reverse: None,
        }));
        pairs.sort_by(|a, b| a.forward.cmp(&b.forward));
        pairs
    }
}

/// Whether the name has a FASTQ extension.
fn is_fastq(file: &str) -> bool {
    let name = file.to_ascii_lowercase();
    [".fastq", ".fq", ".fastq.gz", ".fq.gz"]
        .iter()
        .any(|extension| name.ends_with(extension))
}

/// Last path component.
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Name without the FASTQ extension.
fn strip_extension(name: &str) -> &str {
    for extension in [".fastq.gz", ".fq.gz", ".fastq", ".fq"] {
        if let Some(stem) = name.strip_suffix(extension) {
            return stem;
        }
    }
    name
}

/// Sample name from the part of the path before the read marker.
fn sample_name(prefix: &str) -> String {
    let name = basename(prefix);
    SAMPLE_SUFFIX.replace(name, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    /// Illumina and simple naming pair up, sample names lose the `_S1_L001` part.
    #[test]
    fn pairs_paired_end_files() {
        let pairs = ReadPair::pair(&files(&[
            "reads/A_S1_L001_R2_001.fastq.gz",
            "reads/A_S1_L001_R1_001.fastq.gz",
            "reads/b_1.fq.gz",
            "reads/b_2.fq.gz",
        ]));
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].sample, "A");
        assert_eq!(pairs[0].forward, "reads/A_S1_L001_R1_001.fastq.gz");
        assert_eq!(
            pairs[0].reverse.as_deref(),
            Some("reads/A_S1_L001_R2_001.fastq.gz")
        );
        assert_eq!(pairs[1].sample, "b");
        assert_eq!(pairs[1].reverse.as_deref(), Some("reads/b_2.fq.gz"));
    }

    /// Single-end files, a lone read 2 and the same name in two folders stay separate;
    /// non-FASTQ files are skipped.
    #[test]
    fn keeps_unpaired_and_separate_folders() {
        let pairs = ReadPair::pair(&files(&[
            "x/solo.fastq.gz",
            "x/c_2.fq",
            "d1/s_1.fq",
            "d1/s_2.fq",
            "d2/s_1.fq",
            "notes.txt",
        ]));
        let summary: Vec<(&str, bool)> = pairs
            .iter()
            .map(|p| (p.forward.as_str(), p.reverse.is_some()))
            .collect();
        assert_eq!(
            summary,
            [
                ("d1/s_1.fq", true),
                ("d2/s_1.fq", false),
                ("x/c_2.fq", false),
                ("x/solo.fastq.gz", false)
            ]
        );
        assert_eq!(pairs[3].sample, "solo");
        assert_eq!(pairs[2].sample, "c_2");
    }
}
