//! Select a small, structurally diverse set of local JSON fixtures from WARC files.
//!
//! The first argument is an empty output directory. Each remaining argument is a WARC file or a
//! directory containing WARC files. Sources are sampled independently so one corpus cannot crowd
//! out the others.

use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};

use archivindex_warc::io::read::WarcReader;
use archivindex_warc::record::extension::NoExtension;
use serde_json::Value;

const MAX_FILES_PER_SOURCE: usize = 16;
const MAX_JSON_PER_FILE: usize = 500;
const MAX_PER_KIND_PER_SOURCE: usize = 6;
const MIN_PER_KIND_PER_SOURCE: usize = 2;

struct Fixture {
    source: usize,
    kind: &'static str,
    host: String,
    payload: Value,
}

#[derive(Default)]
struct Selector {
    fixtures: Vec<Fixture>,
    counts: BTreeMap<(usize, &'static str), usize>,
    shapes: BTreeSet<(usize, &'static str, String)>,
}

fn route(target: &str) -> Option<String> {
    let url = url::Url::parse(target).ok()?;
    let start = url.path().find("/wp-json")?;
    Some(url.path()[start..].trim_end_matches('/').to_owned())
}

fn model_kind(route: &str, value: &Value) -> Option<&'static str> {
    if value.get("code").is_some() {
        return Some("errors");
    }

    match route {
        "/wp-json" => Some("api-root"),
        "/wp-json/wp/v2" => Some("namespace"),
        "/wp-json/wp/v2/advanced_ads"
        | "/wp-json/wp/v2/advanced_ads_plcmnt"
        | "/wp-json/wp/v2/blocks"
        | "/wp-json/wp/v2/envira-gallery"
        | "/wp-json/wp/v2/gulag"
        | "/wp-json/wp/v2/jp_pay_order"
        | "/wp-json/wp/v2/jp_pay_product"
        | "/wp-json/wp/v2/navigation"
        | "/wp-json/wp/v2/posts"
        | "/wp-json/wp/v2/videos" => Some("posts"),
        "/wp-json/wp/v2/pages" => Some("pages"),
        "/wp-json/wp/v2/advanced_ads_groups"
        | "/wp-json/wp/v2/categories"
        | "/wp-json/wp/v2/wp_pattern_category" => Some("categories"),
        "/wp-json/wp/v2/tags" => Some("tags"),
        "/wp-json/wp/v2/users" => Some("users"),
        "/wp-json/wp/v2/comments" => Some("comments"),
        "/wp-json/wp/v2/media" => Some("media"),
        "/wp-json/wp/v2/types" => Some("types"),
        "/wp-json/wp/v2/taxonomies" => Some("taxonomies"),
        _ => None,
    }
}

fn shape(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(_) => "bool".to_owned(),
        Value::Number(_) => "number".to_owned(),
        Value::String(_) => "string".to_owned(),
        Value::Array(values) => {
            let shapes = values.iter().map(shape).collect::<BTreeSet<_>>();
            format!("[{}]", shapes.into_iter().collect::<Vec<_>>().join("|"))
        }
        Value::Object(object) => format!(
            "{{{}}}",
            object
                .iter()
                .map(|(key, value)| format!("{key}:{}", shape(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn is_warc(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("warc"))
        || (path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
            && path
                .file_stem()
                .and_then(|stem| Path::new(stem).extension())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("warc")))
}

fn files(path: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    if path.is_file() {
        return Ok(vec![path.to_owned()]);
    }

    let mut pending = vec![path.to_owned()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if is_warc(&path) {
                files.push(path);
            }
        }
    }
    files.sort();

    if files.len() <= MAX_FILES_PER_SOURCE {
        return Ok(files);
    }

    let last = files.len() - 1;
    Ok((0..MAX_FILES_PER_SOURCE)
        .map(|index| files[index * last / (MAX_FILES_PER_SOURCE - 1)].clone())
        .collect())
}

impl Selector {
    fn consider(&mut self, source: usize, host: &str, route: &str, payload: Value) {
        let Some(kind) = model_kind(route, &payload) else {
            return;
        };
        let count = self.counts.entry((source, kind)).or_default();
        if *count >= MAX_PER_KIND_PER_SOURCE {
            return;
        }

        let signature = shape(&payload);
        let novel = self.shapes.insert((source, kind, signature));
        if *count < MIN_PER_KIND_PER_SOURCE || novel {
            self.fixtures.push(Fixture {
                source,
                kind,
                host: host.to_owned(),
                payload,
            });
            *count += 1;
        }
    }

    fn scan_file(&mut self, source: usize, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        eprintln!("sampling {}", path.display());
        if path.extension().is_some_and(|extension| extension == "gz") {
            self.scan_reader(source, WarcReader::from_path_gzip(path)?)
        } else {
            self.scan_reader(source, WarcReader::from_path(path)?)
        }
    }

    fn scan_reader<R: BufRead>(
        &mut self,
        source: usize,
        reader: WarcReader<R>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut json_payloads = 0;
        for result in reader.iter_records::<NoExtension>().records() {
            let record = result?;
            let Some(target_uri) = record.target_uri() else {
                continue;
            };
            let Some(route) = route(target_uri.as_str()) else {
                continue;
            };
            let Some(payload) = record.payload_bytes()? else {
                continue;
            };
            let Ok(payload) = serde_json::from_slice::<Value>(&payload) else {
                continue;
            };
            json_payloads += 1;
            let host = url::Url::parse(target_uri.as_str())
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_else(|| "unknown-host".to_owned());
            self.consider(source, &host, &route, payload);
            if json_payloads >= MAX_JSON_PER_FILE {
                break;
            }
        }

        Ok(())
    }

    fn write(self, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if output.exists() && std::fs::read_dir(output)?.next().is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("output directory is not empty: {}", output.display()),
            )
            .into());
        }
        std::fs::create_dir_all(output)?;

        let mut sequences = BTreeMap::<&str, usize>::new();
        for fixture in self.fixtures {
            let sequence = sequences.entry(fixture.kind).or_default();
            *sequence += 1;
            let directory = output.join(fixture.kind);
            std::fs::create_dir_all(&directory)?;
            let safe_host = fixture
                .host
                .replace(|character: char| !character.is_ascii_alphanumeric(), "-");
            let path = directory.join(format!(
                "{:03}-source-{}-{safe_host}.json",
                sequence, fixture.source
            ));
            std::fs::write(path, serde_json::to_vec_pretty(&fixture.payload)?)?;
        }

        let total = sequences.values().sum::<usize>();
        eprintln!("wrote {total} fixtures to {}", output.display());
        for (kind, count) in sequences {
            eprintln!("  {kind}: {count}");
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let output = arguments.next().map(PathBuf::from).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing output directory")
    })?;
    let sources = arguments.map(PathBuf::from).collect::<Vec<_>>();
    if sources.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "at least one WARC source is required",
        )
        .into());
    }

    let mut selector = Selector::default();
    for (source, path) in sources.iter().enumerate() {
        for file in files(path)? {
            if let Err(error) = selector.scan_file(source, &file) {
                eprintln!("skipping {}: {error}", file.display());
            }
        }
    }
    selector.write(&output)
}
