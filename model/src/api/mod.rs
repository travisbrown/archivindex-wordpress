use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// A `{ "rendered": … }` value, as returned for titles, content, and excerpts.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rendered {
    pub rendered: String,
    #[serde(default)]
    pub protected: bool,
}

/// Link relations included in `WordPress` REST API resources.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Links {
    #[serde(default)]
    pub about: Vec<Link>,
    #[serde(default)]
    pub author: Vec<Link>,
    #[serde(default)]
    pub children: Vec<Link>,
    #[serde(default)]
    pub collection: Vec<Link>,
    #[serde(default)]
    pub curies: Vec<Link>,
    #[serde(default)]
    pub help: Vec<Link>,
    #[serde(default, rename = "in-reply-to")]
    pub in_reply_to: Vec<Link>,
    #[serde(default, rename = "predecessor-version")]
    pub predecessor_version: Vec<Link>,
    #[serde(default)]
    pub replies: Vec<Link>,
    #[serde(default, rename = "self")]
    pub self_links: Vec<Link>,
    #[serde(default)]
    pub up: Vec<Link>,
    #[serde(default, rename = "version-history")]
    pub version_history: Vec<Link>,
    #[serde(default, rename = "wp:attached-to")]
    pub wp_attached_to: Vec<Link>,
    #[serde(default, rename = "wp:attachment")]
    pub wp_attachment: Vec<Link>,
    #[serde(default, rename = "wp:featuredmedia")]
    pub wp_featured_media: Vec<Link>,
    #[serde(default, rename = "wp:items")]
    pub wp_items: Vec<Link>,
    #[serde(default, rename = "wp:post_type")]
    pub wp_post_type: Vec<Link>,
    #[serde(default, rename = "wp:term")]
    pub wp_term: Vec<Link>,
}

/// One entry in a `WordPress` REST API link relation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub href: String,
    #[serde(default)]
    pub embeddable: Option<bool>,
    #[serde(default)]
    pub templated: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub post_type: Option<String>,
    #[serde(default)]
    pub taxonomy: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default, rename = "targetHints")]
    pub target_hints: Option<TargetHints>,
}

/// HTTP methods advertised for a link target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetHints {
    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Post {
    pub id: u64,
    pub date: Option<NaiveDateTime>,
    pub date_gmt: Option<NaiveDateTime>,
    pub modified: Option<NaiveDateTime>,
    pub modified_gmt: Option<NaiveDateTime>,
    pub slug: String,
    pub status: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub link: String,
    #[serde(default)]
    pub title: Rendered,
    #[serde(default)]
    pub content: Rendered,
    #[serde(default)]
    pub excerpt: Rendered,
    #[serde(default)]
    pub guid: Rendered,
    #[serde(default)]
    pub author: u64,
    #[serde(default)]
    pub featured_media: u64,
    #[serde(default)]
    pub comment_status: String,
    #[serde(default)]
    pub ping_status: String,
    #[serde(default)]
    pub sticky: bool,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub categories: Vec<u64>,
    #[serde(default)]
    pub tags: Vec<u64>,
    #[serde(default)]
    pub meta: Value,
    #[serde(default, rename = "_links")]
    pub links: Links,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub class_list: Vec<String>,
    #[serde(default)]
    pub yoast_head: Option<String>,
    #[serde(default)]
    pub yoast_head_json: Value,
    #[serde(default)]
    pub audio_player: Value,
    #[serde(default)]
    pub download_link: Value,
    #[serde(default)]
    pub episode_data: Value,
    #[serde(default)]
    pub episode_featured_image: Value,
    #[serde(default)]
    pub episode_player_image: Value,
    #[serde(default)]
    pub player_link: Value,
    #[serde(default)]
    pub series: Value,
    #[serde(default, rename = "jetpack-related-posts")]
    pub jetpack_related_posts: Value,
    #[serde(default)]
    pub jetpack_featured_media_url: Option<String>,
    #[serde(default)]
    pub jetpack_publicize_connections: Value,
    #[serde(default)]
    pub advanced_ads_groups: Value,
    #[serde(default)]
    pub gallery_data: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: u64,
    pub date: Option<NaiveDateTime>,
    pub date_gmt: Option<NaiveDateTime>,
    pub modified: Option<NaiveDateTime>,
    pub modified_gmt: Option<NaiveDateTime>,
    pub slug: String,
    pub status: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub link: String,
    #[serde(default)]
    pub title: Rendered,
    #[serde(default)]
    pub content: Rendered,
    #[serde(default)]
    pub excerpt: Rendered,
    #[serde(default)]
    pub guid: Rendered,
    pub author: u64,
    #[serde(default)]
    pub parent: u64,
    #[serde(default)]
    pub menu_order: i64,
    #[serde(default)]
    pub featured_media: u64,
    #[serde(default)]
    pub comment_status: String,
    #[serde(default)]
    pub ping_status: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub meta: Value,
    #[serde(default, rename = "_links")]
    pub links: Links,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub class_list: Vec<String>,
    #[serde(default)]
    pub yoast_head: Option<String>,
    #[serde(default)]
    pub yoast_head_json: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: u64,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub description: String,
    pub link: String,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub taxonomy: String,
    #[serde(default)]
    pub parent: u64,
    #[serde(default)]
    pub meta: Value,
    #[serde(default, rename = "_links")]
    pub links: Links,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub yoast_head: Option<String>,
    #[serde(default)]
    pub yoast_head_json: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tag {
    pub id: u64,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub description: String,
    pub link: String,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub taxonomy: String,
    #[serde(default)]
    pub meta: Value,
    #[serde(default, rename = "_links")]
    pub links: Links,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub yoast_head: Option<String>,
    #[serde(default)]
    pub yoast_head_json: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct User {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub link: String,
    pub slug: String,
    #[serde(default)]
    pub avatar_urls: BTreeMap<String, String>,
    #[serde(default)]
    pub is_super_admin: Option<bool>,
    #[serde(default)]
    pub meta: Value,
    #[serde(default, rename = "_links")]
    pub links: Links,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub mpp_avatar: Value,
    #[serde(default)]
    pub woocommerce_meta: Value,
    #[serde(default)]
    pub yoast_head: Option<String>,
    #[serde(default)]
    pub yoast_head_json: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    pub id: u64,
    pub post: u64,
    #[serde(default)]
    pub parent: u64,
    #[serde(default)]
    pub author: u64,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub author_url: String,
    pub date: Option<NaiveDateTime>,
    pub date_gmt: Option<NaiveDateTime>,
    #[serde(default)]
    pub content: Rendered,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub status: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub author_avatar_urls: BTreeMap<String, String>,
    #[serde(default)]
    pub meta: Value,
    #[serde(default, rename = "_links")]
    pub links: Links,
    #[serde(default)]
    pub acf: Value,
}

/// `has_archive` is either a flag or the archive's slug.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum HasArchive {
    Enabled(bool),
    Slug(String),
}

/// Authentication methods advertised by the REST API root.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Authentication {
    #[serde(default, rename = "application-passwords")]
    pub application_passwords: Option<ApplicationPasswords>,
}

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Type {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub hierarchical: bool,
    #[serde(default)]
    pub has_archive: Option<HasArchive>,
    #[serde(default)]
    pub taxonomies: Vec<String>,
    #[serde(default)]
    pub rest_base: String,
    #[serde(default)]
    pub rest_namespace: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub template: Value,
    #[serde(default)]
    pub template_lock: Option<bool>,
    #[serde(default, rename = "_links")]
    pub links: Links,
    #[serde(default)]
    pub yoast_head: Option<String>,
    #[serde(default)]
    pub yoast_head_json: Value,
}

/// The discovery document at the REST API root.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiRoot {
    pub name: String,
    pub description: String,
    pub url: String,
    pub home: String,
    pub gmt_offset: f64,
    pub timezone_string: String,
    pub namespaces: Vec<String>,
    #[serde(default)]
    pub authentication: Authentication,
    #[serde(default)]
    pub routes: BTreeMap<String, Value>,
    #[serde(default)]
    pub site_logo: u64,
    #[serde(default)]
    pub site_icon: u64,
    #[serde(default)]
    pub site_icon_url: String,
    #[serde(default)]
    pub page_for_posts: u64,
    #[serde(default)]
    pub page_on_front: u64,
    #[serde(default)]
    pub show_on_front: String,
    #[serde(default, rename = "_links")]
    pub links: Links,
}

/// A REST API namespace discovery document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Namespace {
    pub namespace: String,
    #[serde(default)]
    pub routes: BTreeMap<String, Value>,
    #[serde(default, rename = "_links")]
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
    #[serde(default)]
    pub data: Option<ErrorData>,
}

/// A taxonomy advertised by the `/wp/v2/taxonomies` registry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Taxonomy {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub hierarchical: bool,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub rest_base: String,
    #[serde(default)]
    pub rest_namespace: String,
    #[serde(default, rename = "_links")]
    pub links: Links,
}

/// An attachment returned by the `/wp/v2/media` collection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Media {
    pub id: u64,
    pub date: Option<NaiveDateTime>,
    pub date_gmt: Option<NaiveDateTime>,
    pub modified: Option<NaiveDateTime>,
    pub modified_gmt: Option<NaiveDateTime>,
    pub slug: String,
    pub status: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub link: String,
    #[serde(default)]
    pub title: Rendered,
    #[serde(default)]
    pub guid: Rendered,
    #[serde(default)]
    pub author: u64,
    #[serde(default)]
    pub featured_media: u64,
    #[serde(default)]
    pub comment_status: String,
    #[serde(default)]
    pub ping_status: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub meta: Value,
    #[serde(default)]
    pub class_list: Vec<String>,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub description: Rendered,
    #[serde(default)]
    pub caption: Rendered,
    #[serde(default)]
    pub alt_text: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub media_details: Value,
    #[serde(default)]
    pub post: Option<u64>,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub filesize: Option<u64>,
    #[serde(default, rename = "_links")]
    pub links: Links,
}

macro_rules! timestamp {
    ($type:ty) => {
        impl $type {
            /// The publication time, from the API's UTC field.
            pub fn timestamp(&self) -> Option<DateTime<Utc>> {
                self.date_gmt.map(|value| Utc.from_utc_datetime(&value))
            }
        }
    };
}

timestamp!(Post);
timestamp!(Page);
timestamp!(Comment);
timestamp!(Media);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ApiRoot, ErrorResponse, Media, Namespace, Post, Taxonomy};

    #[test]
    fn api_discovery_documents_and_errors_parse() {
        let root = serde_json::from_value::<ApiRoot>(json!({
            "name": "Example",
            "description": "An example site",
            "url": "https://example.com",
            "home": "https://example.com",
            "gmt_offset": -4,
            "timezone_string": "America/New_York",
            "namespaces": ["wp/v2"],
            "authentication": {
                "application-passwords": {
                    "endpoints": {
                        "authorization": "https://example.com/wp-admin/authorize-application.php"
                    }
                }
            },
            "routes": {},
            "_links": {
                "help": [{
                    "href": "https://developer.wordpress.org/rest-api/",
                    "targetHints": {"allow": ["GET"]}
                }]
            }
        }))
        .expect("an API root");
        assert_eq!(
            root.links.help[0]
                .target_hints
                .as_ref()
                .expect("target hints")
                .allow,
            ["GET"]
        );
        assert_eq!(
            root.authentication
                .application_passwords
                .expect("application-password authentication")
                .endpoints
                .authorization,
            "https://example.com/wp-admin/authorize-application.php"
        );
        serde_json::from_value::<Namespace>(json!({
            "namespace": "wp/v2",
            "routes": {},
            "_links": {}
        }))
        .expect("a namespace index");
        let error = serde_json::from_value::<ErrorResponse>(json!({
            "code": "rest_cannot_access",
            "message": "Sorry, you are not allowed to do that.",
            "data": {"status": 401}
        }))
        .expect("an API error");
        assert_eq!(error.data.expect("error data").status, 401);
    }

    #[test]
    fn taxonomy_registry_entries_parse() {
        let taxonomy = serde_json::from_value::<Taxonomy>(json!({
            "name": "Categories",
            "slug": "category",
            "description": "",
            "hierarchical": true,
            "types": ["post"],
            "rest_base": "categories",
            "rest_namespace": "wp/v2",
            "_links": {}
        }))
        .expect("a taxonomy");

        assert_eq!(taxonomy.types, ["post"]);
    }

    #[test]
    fn media_attachments_parse() {
        let media = serde_json::from_value::<Media>(json!({
            "id": 42,
            "date": "2026-08-28T17:57:06",
            "date_gmt": "2026-08-28T21:57:06",
            "modified": "2026-08-28T17:57:06",
            "modified_gmt": "2026-08-28T21:57:06",
            "slug": "example-image",
            "status": "inherit",
            "type": "attachment",
            "link": "https://example.com/example-image/",
            "title": {"rendered": "Example image"},
            "guid": {"rendered": "https://example.com/example.png"},
            "author": 1,
            "description": {"rendered": ""},
            "caption": {"rendered": ""},
            "media_type": "image",
            "mime_type": "image/png",
            "media_details": {"width": 1000, "height": 523, "sizes": {}},
            "post": null,
            "source_url": "https://example.com/example.png",
            "filesize": null,
            "_links": {}
        }))
        .expect("a media attachment");

        assert_eq!(
            media.timestamp().expect("a timestamp").to_rfc3339(),
            "2026-08-28T21:57:06+00:00"
        );
    }

    #[test]
    fn custom_post_types_parse_through_the_post_model() {
        let post = serde_json::from_value::<Post>(json!({
            "id": 7,
            "date": "2026-08-28T17:57:06",
            "date_gmt": "2026-08-28T21:57:06",
            "modified": "2026-08-28T17:57:06",
            "modified_gmt": "2026-08-28T21:57:06",
            "slug": "gallery",
            "status": "publish",
            "type": "envira",
            "link": "https://example.com/gallery/",
            "title": {"rendered": "Gallery"},
            "gallery_data": {"id": 7},
            "acf": [],
            "_links": {}
        }))
        .expect("a custom post type");

        assert_eq!(post.author, 0);
    }
}
