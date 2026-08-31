//! Validate the `WordPress` REST API JSON captured in WARC files against the model crate.
//!
//! Set `SHOW_SHAPES` to report the observed field names and JSON types for each route.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::File;
use std::io::BufReader;

use archivindex_warc::io::read::WarcReader;
use archivindex_warc::record::extension::NoExtension;
use archivindex_wordpress_model::api::{
    ApiRoot, Category, Comment, ErrorResponse, Media, Namespace, Page, Post, Tag, Taxonomy, Type,
    User,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Default)]
struct Shape {
    payloads: usize,
    arrays: usize,
    objects: usize,
    other: usize,
    empty_arrays: usize,
    keys: BTreeMap<String, BTreeSet<&'static str>>,
    entry_keys: BTreeMap<String, BTreeSet<&'static str>>,
}

#[derive(Default)]
struct Corpus {
    shapes: BTreeMap<String, Shape>,
    records: usize,
    json_payloads: usize,
    validation: BTreeMap<String, BTreeMap<String, usize>>,
    unhandled: BTreeMap<String, usize>,
}

const fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn add_keys(
    keys: &mut BTreeMap<String, BTreeSet<&'static str>>,
    object: &serde_json::Map<String, Value>,
) {
    for (name, value) in object {
        keys.entry(name.clone()).or_default().insert(kind(value));
    }
}

fn route(target: &str) -> Option<String> {
    let url = url::Url::parse(target).ok()?;
    let start = url.path().find("/wp-json")?;
    Some(url.path()[start..].trim_end_matches('/').to_owned())
}

fn parse<T: DeserializeOwned>(value: &Value) -> Result<(), serde_json::Error> {
    serde_json::from_value::<T>(value.clone()).map(|_| ())
}

fn validate(route: &str, value: &Value) -> Option<Result<(), serde_json::Error>> {
    if value.get("code").is_some() {
        return Some(parse::<ErrorResponse>(value));
    }

    match route {
        "/wp-json" => Some(parse::<ApiRoot>(value)),
        "/wp-json/wp/v2" => Some(parse::<Namespace>(value)),
        "/wp-json/wp/v2/advanced_ads"
        | "/wp-json/wp/v2/advanced_ads_plcmnt"
        | "/wp-json/wp/v2/blocks"
        | "/wp-json/wp/v2/envira-gallery"
        | "/wp-json/wp/v2/gulag"
        | "/wp-json/wp/v2/jp_pay_order"
        | "/wp-json/wp/v2/jp_pay_product"
        | "/wp-json/wp/v2/navigation"
        | "/wp-json/wp/v2/posts"
        | "/wp-json/wp/v2/videos" => Some(parse::<Vec<Post>>(value)),
        "/wp-json/wp/v2/pages" => Some(parse::<Vec<Page>>(value)),
        "/wp-json/wp/v2/advanced_ads_groups"
        | "/wp-json/wp/v2/categories"
        | "/wp-json/wp/v2/wp_pattern_category" => Some(parse::<Vec<Category>>(value)),
        "/wp-json/wp/v2/tags" => Some(parse::<Vec<Tag>>(value)),
        "/wp-json/wp/v2/users" => Some(parse::<Vec<User>>(value)),
        "/wp-json/wp/v2/comments" => Some(parse::<Vec<Comment>>(value)),
        "/wp-json/wp/v2/media" => Some(parse::<Vec<Media>>(value)),
        "/wp-json/wp/v2/types" => Some(parse::<BTreeMap<String, Type>>(value)),
        "/wp-json/wp/v2/taxonomies" => Some(parse::<BTreeMap<String, Taxonomy>>(value)),
        _ => None,
    }
}

impl Corpus {
    fn scan(&mut self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        eprintln!("reading {path}");
        let reader = WarcReader::new(BufReader::new(File::open(path)?));
        for result in reader.iter_records::<NoExtension>().records() {
            self.records += 1;
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
            let Ok(value) = serde_json::from_slice::<Value>(&payload) else {
                continue;
            };
            self.json_payloads += 1;
            match validate(&route, &value) {
                Some(Ok(())) => {}
                Some(Err(error)) => {
                    *self
                        .validation
                        .entry(route.clone())
                        .or_default()
                        .entry(error.to_string())
                        .or_default() += 1;
                }
                None => *self.unhandled.entry(route.clone()).or_default() += 1,
            }
            let shape = self.shapes.entry(route).or_default();
            shape.payloads += 1;
            match value {
                Value::Array(values) => {
                    shape.arrays += 1;
                    shape.empty_arrays += usize::from(values.is_empty());
                    for value in values {
                        if let Value::Object(object) = value {
                            add_keys(&mut shape.keys, &object);
                        }
                    }
                }
                Value::Object(object) => {
                    shape.objects += 1;
                    add_keys(&mut shape.keys, &object);
                    for value in object.values() {
                        if let Value::Object(entry) = value {
                            add_keys(&mut shape.entry_keys, entry);
                        }
                    }
                }
                _ => shape.other += 1,
            }
        }

        Ok(())
    }

    fn report(self) -> Result<(), Box<dyn std::error::Error>> {
        eprintln!(
            "read {} records and {} WordPress JSON payloads",
            self.records, self.json_payloads
        );
        if env::var_os("SHOW_SHAPES").is_some() {
            for (route, shape) in self.shapes {
                println!(
                    "\n{route} payloads={} arrays={} (empty={}) objects={} other={}",
                    shape.payloads, shape.arrays, shape.empty_arrays, shape.objects, shape.other
                );
                println!("fields:");
                for (name, types) in shape.keys {
                    println!(
                        "  {name}: {}",
                        types.into_iter().collect::<Vec<_>>().join("|")
                    );
                }
                if !shape.entry_keys.is_empty() {
                    println!("object entry fields:");
                    for (name, types) in shape.entry_keys {
                        println!(
                            "  {name}: {}",
                            types.into_iter().collect::<Vec<_>>().join("|")
                        );
                    }
                }
            }
        }

        if self.validation.is_empty() && self.unhandled.is_empty() {
            println!(
                "all {} WordPress JSON payloads matched the model",
                self.json_payloads
            );
        } else {
            println!("\nmodel validation failures:");
            for (route, errors) in &self.validation {
                println!("{route}");
                for (error, count) in errors {
                    println!("  {count}: {error}");
                }
            }
            println!("unhandled JSON payloads:");
            for (route, count) in &self.unhandled {
                println!("  {count}: {route}");
            }

            return Err(std::io::Error::other("the model did not parse every JSON payload").into());
        }

        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut corpus = Corpus::default();
    for path in env::args().skip(1) {
        corpus.scan(&path)?;
    }

    corpus.report()
}
