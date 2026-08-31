//! Conditional validation of uncommitted local response fixtures.

use std::collections::BTreeMap;
use std::path::Path;

use archivindex_wordpress_model::api::{
    ApiRoot, Category, Comment, ErrorResponse, Media, Namespace, Page, Post, Tag, Taxonomy, Type,
    User,
};
use serde::de::DeserializeOwned;

fn parse<T: DeserializeOwned>(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::from_slice::<T>(&std::fs::read(path)?)?;
    Ok(())
}

fn parse_fixture(kind: &str, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    match kind {
        "api-root" => parse::<ApiRoot>(path),
        "namespace" => parse::<Namespace>(path),
        "errors" => parse::<ErrorResponse>(path),
        "posts" => parse::<Vec<Post>>(path),
        "pages" => parse::<Vec<Page>>(path),
        "categories" => parse::<Vec<Category>>(path),
        "tags" => parse::<Vec<Tag>>(path),
        "users" => parse::<Vec<User>>(path),
        "comments" => parse::<Vec<Comment>>(path),
        "media" => parse::<Vec<Media>>(path),
        "types" => parse::<BTreeMap<String, Type>>(path),
        "taxonomies" => parse::<BTreeMap<String, Taxonomy>>(path),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown local fixture kind: {kind}"),
        )
        .into()),
    }
}

#[test]
fn local_response_fixtures_match_the_model() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/data/local");
    if !root.is_dir() {
        return Ok(());
    }

    let mut fixtures = 0;
    for directory in std::fs::read_dir(&root)? {
        let directory = directory?;
        if !directory.file_type()?.is_dir() {
            continue;
        }
        let kind = directory.file_name();
        let kind = kind.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "non-UTF-8 fixture kind")
        })?;
        for fixture in std::fs::read_dir(directory.path())? {
            let fixture = fixture?;
            let path = fixture.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            fixtures += 1;
            parse_fixture(kind, &path).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: {error}", path.display()),
                )
            })?;
        }
    }

    assert!(fixtures > 0, "local fixture directory is empty");
    Ok(())
}
