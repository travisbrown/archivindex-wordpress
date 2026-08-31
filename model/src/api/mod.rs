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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub links: Value,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub class_list: Value,
    #[serde(default)]
    pub yoast_head: Value,
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
    pub jetpack_featured_media_url: Value,
    #[serde(default)]
    pub jetpack_publicize_connections: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub links: Value,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub class_list: Value,
    #[serde(default)]
    pub yoast_head: Value,
    #[serde(default)]
    pub yoast_head_json: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub links: Value,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub yoast_head: Value,
    #[serde(default)]
    pub yoast_head_json: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub links: Value,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub yoast_head: Value,
    #[serde(default)]
    pub yoast_head_json: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub links: Value,
    #[serde(default)]
    pub acf: Value,
    #[serde(default)]
    pub mpp_avatar: Value,
    #[serde(default)]
    pub woocommerce_meta: Value,
    #[serde(default)]
    pub yoast_head: Value,
    #[serde(default)]
    pub yoast_head_json: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub links: Value,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub icon: Value,
    #[serde(default)]
    pub template: Value,
    #[serde(default)]
    pub template_lock: Value,
    #[serde(default, rename = "_links")]
    pub links: Value,
    #[serde(default)]
    pub yoast_head: Value,
    #[serde(default)]
    pub yoast_head_json: Value,
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
