//! Combining the continuation segments of one site's collection archive.
//!
//! The first warcinfo record is retained, later warcinfo records are removed, and references to
//! their identifiers are redirected to the retained record. Its declared filename is updated to
//! the output basename.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufWriter, Read};
use std::path::{Path, PathBuf};

use archivindex_warc::io::read::WarcReader;
use archivindex_warc::io::write::{Compression, WarcWriter};
use archivindex_warc::parse::raw;
use archivindex_warc::value::Text;

/// Header fields whose values identify another WARC record.
const REFERENCE_FIELDS: [&str; 4] = [
    "WARC-Warcinfo-ID",
    "WARC-Refers-To",
    "WARC-Concurrent-To",
    "WARC-Segment-Origin-ID",
];

/// Options for combining a site's archive segments.
#[derive(Debug, clap::Args)]
pub struct CombineOptions {
    /// Directory containing the archive and resume-run WARC files.
    #[arg(short, long, value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    pub(super) input: PathBuf,
    /// Domain prefix used by the archive session names, such as `example.com`.
    #[arg(long, value_name = "DOMAIN")]
    pub(super) domain: String,
    /// New WARC to write; `.warc.gz` is gzip-compressed and `.warc` is uncompressed.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub(super) output: PathBuf,
}

/// What a successful combination wrote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombineSummary {
    pub(super) files: usize,
    pub(super) records: usize,
}

/// The ways archive segments can fail to combine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read combine input directory {}: {source}", path.display())]
    InputDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("combine input directory {} contains no direct WARC files for domain {domain:?}", input.display())]
    NoInputs { input: PathBuf, domain: String },
    #[error("combined output {} must end in .warc or .warc.gz", .0.display())]
    OutputExtension(PathBuf),
    #[error(
        "cannot redirect later warcinfo records because the first warcinfo has no WARC-Record-ID"
    )]
    MissingFirstWarcinfoRecordId,
    #[error("cannot create combined output {}: {source}", path.display())]
    OutputCreate {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot inspect archive segment {}: {source}", path.display())]
    InspectInput {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read archive segment {}: {source}", path.display())]
    ReadInput {
        path: PathBuf,
        #[source]
        source: archivindex_warc::io::read::Error,
    },
    #[error("cannot write combined output {}: {source}", path.display())]
    WriteOutput {
        path: PathBuf,
        #[source]
        source: archivindex_warc::io::write::Error,
    },
    #[error("cannot flush combined output {}: {source}", path.display())]
    FlushOutput {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Combine domain-prefixed WARC files in filename order into one WARC.
pub fn combine_archives(options: &CombineOptions) -> Result<CombineSummary, Error> {
    if !has_warc_output_extension(&options.output) {
        return Err(Error::OutputExtension(options.output.clone()));
    }

    let inputs = domain_inputs(&options.input, &options.domain, &options.output)?;
    if inputs.is_empty() {
        return Err(Error::NoInputs {
            input: options.input.clone(),
            domain: options.domain.clone(),
        });
    }

    let plan = WarcinfoPlan::build(&inputs)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&options.output)
        .map_err(|source| Error::OutputCreate {
            path: options.output.clone(),
            source,
        })?;
    let result = write_inputs(&inputs, file, &options.output, &plan);
    if result.is_err()
        && let Err(error) = std::fs::remove_file(&options.output)
    {
        log::warn!(
            "could not remove partial combined output {}: {error}",
            options.output.display()
        );
    }

    result.map(|records| CombineSummary {
        files: inputs.len(),
        records,
    })
}

fn domain_inputs(input: &Path, domain: &str, output: &Path) -> Result<Vec<PathBuf>, Error> {
    let entries = std::fs::read_dir(input).map_err(|source| Error::InputDirectory {
        path: input.to_owned(),
        source,
    })?;
    let prefix = format!("{domain}-");
    let mut paths = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|source| Error::InputDirectory {
            path: input.to_owned(),
            source,
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if path != output && path.is_file() && name.starts_with(&prefix) && has_warc_extension(name)
        {
            paths.push(path);
        }
    }

    paths.sort();
    Ok(paths)
}

fn write_inputs(
    inputs: &[PathBuf],
    file: File,
    output: &Path,
    plan: &WarcinfoPlan,
) -> Result<usize, Error> {
    let compression = if has_warc_gzip_extension(output) {
        Compression::gzip()
    } else {
        Compression::NONE
    };
    let mut writer = WarcWriter::new(BufWriter::new(file)).with_compression(compression);
    let mut records = 0;
    let mut seen_warcinfo = false;
    let filename = output_filename(output);

    for input in inputs {
        let gzip = is_gzip_file(input).map_err(|source| Error::InspectInput {
            path: input.clone(),
            source,
        })?;
        records += if gzip {
            let reader =
                WarcReader::from_path_gzip(input).map_err(|source| Error::InspectInput {
                    path: input.clone(),
                    source,
                })?;
            copy_records(
                reader,
                &mut writer,
                input,
                output,
                plan,
                &mut seen_warcinfo,
                filename.as_deref(),
            )?
        } else {
            let reader = WarcReader::from_path(input).map_err(|source| Error::InspectInput {
                path: input.clone(),
                source,
            })?;
            copy_records(
                reader,
                &mut writer,
                input,
                output,
                plan,
                &mut seen_warcinfo,
                filename.as_deref(),
            )?
        };
    }

    writer.flush().map_err(|source| Error::FlushOutput {
        path: output.to_owned(),
        source,
    })?;
    Ok(records)
}

fn copy_records<R: BufRead>(
    reader: WarcReader<R>,
    writer: &mut WarcWriter<BufWriter<File>>,
    input: &Path,
    output: &Path,
    plan: &WarcinfoPlan,
    seen_warcinfo: &mut bool,
    filename: Option<&[u8]>,
) -> Result<usize, Error> {
    let mut count = 0;
    for record in reader.iter_raw_records().records() {
        let mut record = record.map_err(|source| Error::ReadInput {
            path: input.to_owned(),
            source,
        })?;
        if is_warcinfo(&record.header) {
            if *seen_warcinfo {
                continue;
            }
            *seen_warcinfo = true;
            set_filename(&mut record.header, filename);
        }
        redirect_references(&mut record.header, &plan.redirects);
        writer.write(&record).map_err(|source| Error::WriteOutput {
            path: output.to_owned(),
            source,
        })?;
        count += 1;
    }

    Ok(count)
}

/// The later warcinfo identifiers that must resolve to the first warcinfo record.
struct WarcinfoPlan {
    redirects: HashMap<Vec<u8>, Vec<u8>>,
}

impl WarcinfoPlan {
    /// Read the inputs once to identify the first warcinfo and all later identifiers.
    fn build(inputs: &[PathBuf]) -> Result<Self, Error> {
        let mut first_seen = false;
        let mut first_id = None;
        let mut redirects = HashMap::new();

        for input in inputs {
            let gzip = is_gzip_file(input).map_err(|source| Error::InspectInput {
                path: input.clone(),
                source,
            })?;
            if gzip {
                let reader =
                    WarcReader::from_path_gzip(input).map_err(|source| Error::InspectInput {
                        path: input.clone(),
                        source,
                    })?;
                inspect_warcinfo(
                    reader,
                    input,
                    &mut first_seen,
                    &mut first_id,
                    &mut redirects,
                )?;
            } else {
                let reader =
                    WarcReader::from_path(input).map_err(|source| Error::InspectInput {
                        path: input.clone(),
                        source,
                    })?;
                inspect_warcinfo(
                    reader,
                    input,
                    &mut first_seen,
                    &mut first_id,
                    &mut redirects,
                )?;
            }
        }

        Ok(Self { redirects })
    }
}

fn inspect_warcinfo<R: BufRead>(
    reader: WarcReader<R>,
    input: &Path,
    first_seen: &mut bool,
    first_id: &mut Option<Vec<u8>>,
    redirects: &mut HashMap<Vec<u8>, Vec<u8>>,
) -> Result<(), Error> {
    for record in reader.iter_raw_records().records() {
        let record = record.map_err(|source| Error::ReadInput {
            path: input.to_owned(),
            source,
        })?;
        if !is_warcinfo(&record.header) {
            continue;
        }

        let id = record_id(&record).map(<[u8]>::to_vec);
        if !*first_seen {
            *first_seen = true;
            *first_id = id;
            continue;
        }

        let Some(dropped_id) = id else {
            continue;
        };
        let Some(kept_id) = first_id.as_deref() else {
            return Err(Error::MissingFirstWarcinfoRecordId);
        };
        if normalize_id(&dropped_id) != normalize_id(kept_id) {
            let mut replacement = Vec::with_capacity(kept_id.len() + 1);
            replacement.push(b' ');
            replacement.extend_from_slice(kept_id);
            redirects.insert(normalize_id(&dropped_id).to_vec(), replacement);
        }
    }

    Ok(())
}

fn is_warcinfo(header: &raw::RecordHeader) -> bool {
    header
        .get("WARC-Type")
        .is_some_and(|value| value.trim_ascii().eq_ignore_ascii_case(b"warcinfo"))
}

fn record_id(record: &raw::Record) -> Option<&[u8]> {
    record
        .header
        .get("WARC-Record-ID")
        .map(<[u8]>::trim_ascii)
        .filter(|value| !value.is_empty())
}

fn normalize_id(value: &[u8]) -> &[u8] {
    let value = value.trim_ascii();
    value
        .strip_prefix(b"<")
        .and_then(|inner| inner.strip_suffix(b">"))
        .unwrap_or(value)
}

fn redirect_references(header: &mut raw::RecordHeader, redirects: &HashMap<Vec<u8>, Vec<u8>>) {
    for (name, value) in &mut header.headers {
        if REFERENCE_FIELDS
            .iter()
            .any(|field| name.eq_ignore_ascii_case(field))
            && let Some(replacement) = redirects.get(normalize_id(value))
        {
            value.clone_from(replacement);
        }
    }
}

fn output_filename(output: &Path) -> Option<Vec<u8>> {
    let name = output.file_name()?.to_str()?;
    let text = Text::parse(name.as_bytes()).ok()?;
    let spelled = text.to_bytes();
    let mut value = Vec::with_capacity(spelled.len() + 1);
    value.push(b' ');
    value.extend_from_slice(&spelled);

    Some(value)
}

fn set_filename(header: &mut raw::RecordHeader, filename: Option<&[u8]>) {
    header.headers.retain_mut(|(name, value)| {
        if !name.eq_ignore_ascii_case("WARC-Filename") {
            return true;
        }
        let Some(filename) = filename else {
            return false;
        };
        value.clear();
        value.extend_from_slice(filename);

        true
    });
}

fn is_gzip_file(path: &Path) -> Result<bool, std::io::Error> {
    let mut file = File::open(path)?;
    let mut magic = [0; 2];
    Ok(file.read(&mut magic)? == magic.len() && magic == [0x1f, 0x8b])
}

fn has_warc_extension(name: &str) -> bool {
    let path = Path::new(name);
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("warc")
            || extension.eq_ignore_ascii_case("gz")
                && path
                    .file_stem()
                    .and_then(|stem| Path::new(stem).extension())
                    .is_some_and(|inner| inner.eq_ignore_ascii_case("warc"))
    })
}

fn has_warc_gzip_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
        && path
            .file_stem()
            .and_then(|stem| Path::new(stem).extension())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("warc"))
}

fn has_warc_output_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("warc"))
        || has_warc_gzip_extension(path)
}
