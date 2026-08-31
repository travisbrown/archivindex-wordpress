//! Link relations returned with REST API documents and resources.

use serde::{Deserialize, Serialize};

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
