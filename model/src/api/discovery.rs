//! REST API discovery documents, registries, and errors.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::extensions::NullableYoastMetadata;
use super::{GmtOffset, Links};

/// `has_archive` is either a flag or the archive's slug.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum HasArchive {
    Enabled(bool),
    Slug(String),
}

/// A template lock represented as a flag or a named lock mode.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum TemplateLock {
    Enabled(bool),
    Mode(TemplateLockMode),
}

/// Named template-lock modes observed in post-type registries.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateLockMode {
    All,
}

/// Authentication metadata advertised by the REST API root.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Authentication {
    Methods(AuthenticationMethods),
    Empty(Vec<NoAuthenticationMethod>),
}

/// Authentication methods advertised by newer REST API roots.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticationMethods {
    #[serde(rename = "application-passwords")]
    pub application_passwords: Option<ApplicationPasswords>,
}

/// An uninhabited type that restricts legacy authentication arrays to empty arrays.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum NoAuthenticationMethod {}

/// `WordPress` application-password authentication metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationPasswords {
    pub endpoints: ApplicationPasswordEndpoints,
}

/// Endpoints used to authorize `WordPress` application passwords.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationPasswordEndpoints {
    pub authorization: String,
}

/// A post type advertised by the `/wp/v2/types` registry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Type {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub hierarchical: bool,
    pub has_archive: Option<HasArchive>,
    pub taxonomies: Vec<String>,
    pub rest_base: String,
    pub rest_namespace: Option<String>,
    pub icon: Option<String>,
    pub template: Option<Value>,
    pub template_lock: Option<TemplateLock>,
    #[serde(rename = "_links")]
    pub links: Links,
    #[serde(flatten)]
    pub yoast: NullableYoastMetadata,
}

/// The discovery document at the REST API root.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiRoot {
    pub name: String,
    pub description: String,
    pub url: String,
    pub home: String,
    pub gmt_offset: GmtOffset,
    pub timezone_string: String,
    pub namespaces: Vec<String>,
    pub authentication: Authentication,
    pub routes: BTreeMap<String, Value>,
    pub site_logo: u64,
    pub site_icon: u64,
    pub site_icon_url: String,
    pub page_for_posts: Option<u64>,
    pub page_on_front: Option<u64>,
    pub show_on_front: Option<String>,
    #[serde(rename = "_links")]
    pub links: Links,
}

/// A REST API namespace discovery document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Namespace {
    pub namespace: String,
    pub routes: BTreeMap<String, Value>,
    #[serde(rename = "_links")]
    pub links: Links,
}

/// Machine-readable details attached to a REST API error.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorData {
    pub status: u16,
}

/// An error returned by the REST API.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub data: ErrorData,
}

/// A taxonomy advertised by the `/wp/v2/taxonomies` registry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Taxonomy {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub hierarchical: bool,
    pub types: Vec<String>,
    pub rest_base: String,
    pub rest_namespace: String,
    #[serde(rename = "_links")]
    pub links: Links,
}
