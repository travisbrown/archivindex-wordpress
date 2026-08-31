use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// A `{ "rendered": … }` value, as returned for titles, content, and excerpts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rendered {
    pub rendered: String,
    pub protected: Option<bool>,
}

/// Publication state observed for posts and pages.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PostStatus {
    Publish,
}

/// Moderation state observed for public comments.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentStatus {
    Approved,
}

/// Publication state observed for media attachments.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaStatus {
    Inherit,
}

/// Whether comments or pings are accepted for a resource.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscussionStatus {
    #[serde(rename = "")]
    Unavailable,
    Closed,
    Open,
}

/// Presentation format selected for a post.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PostFormat {
    Aside,
    Standard,
}

/// Broad attachment category reported by the media endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    File,
    Image,
}

/// Resource discriminator returned by the page endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    Page,
}

/// Resource discriminator returned by the comment endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentKind {
    Comment,
}

/// Resource discriminator returned by the media endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Attachment,
}

/// Taxonomy discriminator returned by the category endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryTaxonomy {
    Category,
}

/// Taxonomy discriminator returned by the tag endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TagTaxonomy {
    PostTag,
}

/// Link relations included in `WordPress` REST API resources.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Links {
    pub about: Option<Vec<Link>>,
    pub author: Option<Vec<Link>>,
    pub children: Option<Vec<Link>>,
    pub collection: Option<Vec<Link>>,
    pub curies: Option<Vec<Link>>,
    pub help: Option<Vec<Link>>,
    #[serde(rename = "in-reply-to")]
    pub in_reply_to: Option<Vec<Link>>,
    #[serde(rename = "predecessor-version")]
    pub predecessor_version: Option<Vec<Link>>,
    pub replies: Option<Vec<Link>>,
    #[serde(rename = "self")]
    pub self_links: Option<Vec<Link>>,
    pub up: Option<Vec<Link>>,
    #[serde(rename = "version-history")]
    pub version_history: Option<Vec<Link>>,
    #[serde(rename = "wp:attached-to")]
    pub wp_attached_to: Option<Vec<Link>>,
    #[serde(rename = "wp:attachment")]
    pub wp_attachment: Option<Vec<Link>>,
    #[serde(rename = "wp:featuredmedia")]
    pub wp_featured_media: Option<Vec<Link>>,
    #[serde(rename = "wp:items")]
    pub wp_items: Option<Vec<Link>>,
    #[serde(rename = "wp:post_type")]
    pub wp_post_type: Option<Vec<Link>>,
    #[serde(rename = "wp:term")]
    pub wp_term: Option<Vec<Link>>,
}

/// One entry in a `WordPress` REST API link relation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub href: String,
    pub embeddable: Option<bool>,
    pub templated: Option<bool>,
    pub name: Option<String>,
    pub id: Option<u64>,
    pub count: Option<u64>,
    pub post_type: Option<String>,
    pub taxonomy: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(rename = "targetHints")]
    pub target_hints: Option<TargetHints>,
}

/// HTTP methods advertised for a link target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetHints {
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
    pub status: PostStatus,
    #[serde(rename = "type")]
    pub kind: String,
    pub link: String,
    pub title: Rendered,
    pub content: Option<Rendered>,
    pub excerpt: Option<Rendered>,
    pub guid: Rendered,
    pub author: Option<u64>,
    pub featured_media: Option<u64>,
    pub comment_status: Option<DiscussionStatus>,
    pub ping_status: Option<DiscussionStatus>,
    pub sticky: Option<bool>,
    pub format: Option<PostFormat>,
    pub template: String,
    pub categories: Option<Vec<u64>>,
    pub tags: Option<Vec<u64>>,
    pub meta: Option<Value>,
    #[serde(rename = "_links")]
    pub links: Links,
    pub acf: Option<Value>,
    pub class_list: Option<Vec<String>>,
    pub yoast_head: Option<String>,
    pub yoast_head_json: Option<Value>,
    pub audio_player: Option<Value>,
    pub download_link: Option<Value>,
    pub episode_data: Option<Value>,
    pub episode_featured_image: Option<Value>,
    pub episode_player_image: Option<Value>,
    pub player_link: Option<Value>,
    pub series: Option<Value>,
    #[serde(rename = "jetpack-related-posts")]
    pub jetpack_related_posts: Option<Value>,
    pub jetpack_featured_media_url: Option<String>,
    pub jetpack_publicize_connections: Option<Value>,
    pub advanced_ads_groups: Option<Value>,
    pub gallery_data: Option<Value>,
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
    pub status: PostStatus,
    #[serde(rename = "type")]
    pub kind: PageKind,
    pub link: String,
    pub title: Rendered,
    pub content: Rendered,
    pub excerpt: Rendered,
    pub guid: Rendered,
    pub author: u64,
    pub parent: u64,
    pub menu_order: i64,
    pub featured_media: u64,
    pub comment_status: DiscussionStatus,
    pub ping_status: DiscussionStatus,
    pub template: String,
    pub meta: Value,
    #[serde(rename = "_links")]
    pub links: Links,
    pub acf: Option<Value>,
    pub class_list: Option<Vec<String>>,
    pub yoast_head: Option<String>,
    pub yoast_head_json: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: u64,
    pub count: u64,
    pub description: String,
    pub link: String,
    pub name: String,
    pub slug: String,
    pub taxonomy: CategoryTaxonomy,
    pub parent: u64,
    pub meta: Value,
    #[serde(rename = "_links")]
    pub links: Links,
    pub acf: Option<Value>,
    pub yoast_head: Option<String>,
    pub yoast_head_json: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tag {
    pub id: u64,
    pub count: u64,
    pub description: String,
    pub link: String,
    pub name: String,
    pub slug: String,
    pub taxonomy: TagTaxonomy,
    pub meta: Value,
    #[serde(rename = "_links")]
    pub links: Links,
    pub acf: Option<Value>,
    pub yoast_head: Option<String>,
    pub yoast_head_json: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub description: String,
    pub link: String,
    pub slug: String,
    pub avatar_urls: Option<BTreeMap<String, String>>,
    pub is_super_admin: Option<bool>,
    pub meta: Value,
    #[serde(rename = "_links")]
    pub links: Links,
    pub acf: Option<Value>,
    pub mpp_avatar: Option<Value>,
    pub woocommerce_meta: Option<Value>,
    pub yoast_head: Option<String>,
    pub yoast_head_json: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    pub id: u64,
    pub post: u64,
    pub parent: u64,
    pub author: u64,
    pub author_name: String,
    pub author_url: String,
    pub date: Option<NaiveDateTime>,
    pub date_gmt: Option<NaiveDateTime>,
    pub content: Rendered,
    pub link: String,
    pub status: CommentStatus,
    #[serde(rename = "type")]
    pub kind: CommentKind,
    pub author_avatar_urls: Option<BTreeMap<String, String>>,
    pub meta: Value,
    #[serde(rename = "_links")]
    pub links: Links,
    pub acf: Option<Value>,
}

/// `has_archive` is either a flag or the archive's slug.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum HasArchive {
    Enabled(bool),
    Slug(String),
}

/// Authentication methods advertised by the REST API root.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Authentication {
    #[serde(rename = "application-passwords")]
    pub application_passwords: ApplicationPasswords,
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
    pub description: String,
    pub hierarchical: bool,
    pub has_archive: Option<HasArchive>,
    pub taxonomies: Vec<String>,
    pub rest_base: String,
    pub rest_namespace: String,
    pub icon: Option<String>,
    pub template: Option<Value>,
    pub template_lock: Option<bool>,
    #[serde(rename = "_links")]
    pub links: Links,
    pub yoast_head: Option<String>,
    pub yoast_head_json: Option<Value>,
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
    pub status: MediaStatus,
    #[serde(rename = "type")]
    pub kind: MediaKind,
    pub link: String,
    pub title: Rendered,
    pub guid: Rendered,
    pub author: u64,
    pub featured_media: Option<u64>,
    pub comment_status: DiscussionStatus,
    pub ping_status: DiscussionStatus,
    pub template: String,
    pub meta: Value,
    pub class_list: Option<Vec<String>>,
    pub acf: Option<Value>,
    pub description: Rendered,
    pub caption: Rendered,
    pub alt_text: String,
    pub media_type: MediaType,
    pub mime_type: String,
    pub media_details: Value,
    pub post: Option<u64>,
    pub source_url: String,
    pub filename: Option<String>,
    pub filesize: Option<u64>,
    #[serde(rename = "_links")]
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

    use super::{
        ApiRoot, DiscussionStatus, ErrorResponse, Media, MediaType, Namespace, Post, PostFormat,
        Taxonomy,
    };

    #[test]
    fn bounded_string_fields_parse_as_enums() {
        assert_eq!(
            serde_json::from_str::<DiscussionStatus>(r#""open""#).expect("an open status"),
            DiscussionStatus::Open
        );
        assert_eq!(
            serde_json::from_str::<PostFormat>(r#""aside""#).expect("an aside format"),
            PostFormat::Aside
        );
        assert_eq!(
            serde_json::from_str::<MediaType>(r#""file""#).expect("a file media type"),
            MediaType::File
        );
        assert!(serde_json::from_str::<PostFormat>(r#""unexpected""#).is_err());
    }

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
            "site_logo": 0,
            "site_icon": 0,
            "site_icon_url": "",
            "_links": {
                "help": [{
                    "href": "https://developer.wordpress.org/rest-api/",
                    "targetHints": {"allow": ["GET"]}
                }]
            }
        }))
        .expect("an API root");
        assert_eq!(
            root.links.help.as_ref().expect("help links")[0]
                .target_hints
                .as_ref()
                .expect("target hints")
                .allow,
            ["GET"]
        );
        assert_eq!(
            root.authentication
                .application_passwords
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
        assert_eq!(error.data.status, 401);
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
            "comment_status": "closed",
            "ping_status": "closed",
            "template": "",
            "meta": {},
            "description": {"rendered": ""},
            "caption": {"rendered": ""},
            "alt_text": "",
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
            "guid": {"rendered": "https://example.com/?post_type=envira&p=7"},
            "template": "",
            "gallery_data": {"id": 7},
            "acf": [],
            "_links": {}
        }))
        .expect("a custom post type");

        assert_eq!(post.author, None);
    }
}
