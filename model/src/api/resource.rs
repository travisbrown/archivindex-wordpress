//! Core `WordPress` content, taxonomy, user, and comment resources.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::extensions::{
    AmpUserMetadata, ElementorIntroduction, JetpackRelatedPosts, JetpackSharing,
    NullableYoastMetadata, UagbMetadata, YoastMetadata,
};
use super::{Links, ModificationDates, PublicationDates, Rendered};

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
    Audio,
    Standard,
    Video,
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

/// A post or custom-post-type resource.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Post {
    pub id: u64,
    #[serde(flatten)]
    pub publication: PublicationDates,
    #[serde(flatten)]
    pub modification: ModificationDates,
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
    #[serde(flatten)]
    pub jetpack_related: Option<JetpackRelatedPosts>,
    pub jetpack_featured_media_url: Option<String>,
    #[serde(flatten)]
    pub jetpack_sharing: Option<JetpackSharing>,
    pub advanced_ads_groups: Option<Value>,
    pub gallery_data: Option<Value>,
    #[serde(flatten)]
    pub uagb: Option<UagbMetadata>,
}

impl Post {
    /// The publication time, from the API's UTC field.
    #[must_use]
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.publication.timestamp()
    }
}

/// A page resource.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: u64,
    #[serde(flatten)]
    pub publication: PublicationDates,
    #[serde(flatten)]
    pub modification: ModificationDates,
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
    #[serde(flatten)]
    pub yoast: Option<YoastMetadata>,
    #[serde(flatten)]
    pub jetpack: Option<JetpackSharing>,
    #[serde(flatten)]
    pub uagb: Option<UagbMetadata>,
}

impl Page {
    /// The publication time, from the API's UTC field.
    #[must_use]
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.publication.timestamp()
    }
}

/// A category resource.
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
    #[serde(flatten)]
    pub yoast: Option<YoastMetadata>,
}

/// A tag resource.
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
    #[serde(flatten)]
    pub yoast: Option<YoastMetadata>,
}

/// A user resource.
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
    #[serde(flatten)]
    pub yoast: NullableYoastMetadata,
    #[serde(flatten)]
    pub amp: Option<AmpUserMetadata>,
    pub elementor_introduction: Option<ElementorIntroduction>,
    pub user_switching_url: Option<()>,
}

/// A public comment resource.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    pub id: u64,
    pub post: u64,
    pub parent: u64,
    pub author: u64,
    pub author_name: String,
    pub author_url: String,
    #[serde(flatten)]
    pub publication: PublicationDates,
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

impl Comment {
    /// The publication time, from the API's UTC field.
    #[must_use]
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.publication.timestamp()
    }
}
