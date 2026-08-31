//! Metadata added to core REST resources by common `WordPress` extensions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Author metadata added by Ultimate Addons for Gutenberg.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UagbAuthorInfo {
    pub display_name: String,
    pub author_link: String,
}

/// The UAGB fields that are either all present on a resource or all absent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UagbMetadata {
    #[serde(rename = "uagb_author_info")]
    pub author_info: UagbAuthorInfo,
    #[serde(rename = "uagb_comment_info")]
    pub comment_info: u64,
    #[serde(rename = "uagb_excerpt")]
    pub excerpt: String,
    #[serde(rename = "uagb_featured_image_src")]
    pub featured_image_src: BTreeMap<String, bool>,
}

/// `VideoPress` metadata attached to media resources by Jetpack.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JetpackVideoPress {
    pub title: String,
    pub description: String,
    pub caption: String,
    pub guid: (),
    pub rating: (),
    pub allow_download: u64,
    pub display_embed: u64,
    pub privacy_setting: u64,
    pub needs_playback_token: bool,
    pub is_private: bool,
    pub private_enabled_for_site: bool,
}

/// Jetpack sharing fields that are either both present or both absent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JetpackSharing {
    #[serde(rename = "jetpack_sharing_enabled")]
    pub sharing_enabled: bool,
    #[serde(rename = "jetpack_shortlink")]
    pub shortlink: String,
}

/// Jetpack related-post fields that are either both present or both absent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JetpackRelatedPosts {
    #[serde(rename = "jetpack-related-posts")]
    pub related_posts: Value,
    #[serde(rename = "jetpack_publicize_connections")]
    pub publicize_connections: Value,
}

/// The complete Jetpack extension observed on media resources.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JetpackMedia {
    #[serde(rename = "jetpack_sharing_enabled")]
    pub sharing_enabled: bool,
    #[serde(rename = "jetpack_shortlink")]
    pub shortlink: String,
    #[serde(rename = "jetpack_videopress")]
    pub videopress: JetpackVideoPress,
    #[serde(rename = "jetpack_videopress_guid")]
    pub videopress_guid: String,
}

/// Yoast metadata whose values are present when the extension is advertised.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct YoastMetadata {
    #[serde(rename = "yoast_head")]
    pub head: String,
    #[serde(rename = "yoast_head_json")]
    pub head_json: Value,
}

/// Yoast metadata on resources that may redact both values as `null`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NullableYoastMetadata {
    #[serde(rename = "yoast_head")]
    pub head: Option<String>,
    #[serde(rename = "yoast_head_json")]
    pub head_json: Option<Value>,
}

/// AMP user settings that are either both present or both absent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AmpUserMetadata {
    #[serde(rename = "amp_dev_tools_enabled")]
    pub dev_tools_enabled: bool,
    #[serde(rename = "amp_review_panel_dismissed_for_template_mode")]
    pub review_panel_dismissed_for_template_mode: String,
}

/// Elementor introduction state represented as an empty marker or named flags.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ElementorIntroduction {
    Marker(String),
    Flags(BTreeMap<String, bool>),
}
