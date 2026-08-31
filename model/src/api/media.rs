//! Media attachment resources.

use chrono::{DateTime, Utc};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::extensions::{JetpackMedia, JetpackVideoPress};
use super::{DiscussionStatus, Links, ModificationDates, PublicationDates, Rendered};

/// Publication state observed for media attachments.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaStatus {
    Inherit,
}

/// Broad attachment category reported by the media endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    File,
    Image,
}

/// Resource discriminator returned by the media endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Attachment,
}

/// An attachment returned by the `/wp/v2/media` collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Media {
    pub id: u64,
    #[serde(flatten)]
    pub publication: PublicationDates,
    #[serde(flatten)]
    pub modification: ModificationDates,
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
    pub links: Option<Links>,
    #[serde(flatten)]
    pub jetpack: Option<JetpackMedia>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaWire {
    id: u64,
    #[serde(flatten)]
    publication: PublicationDates,
    #[serde(flatten)]
    modification: ModificationDates,
    slug: String,
    status: MediaStatus,
    #[serde(rename = "type")]
    kind: MediaKind,
    link: String,
    title: Rendered,
    guid: Rendered,
    author: u64,
    featured_media: Option<u64>,
    comment_status: DiscussionStatus,
    ping_status: DiscussionStatus,
    template: String,
    meta: Value,
    class_list: Option<Vec<String>>,
    acf: Option<Value>,
    description: Rendered,
    caption: Rendered,
    alt_text: String,
    media_type: MediaType,
    mime_type: String,
    media_details: Value,
    post: Option<u64>,
    source_url: String,
    filename: Option<String>,
    filesize: Option<u64>,
    #[serde(rename = "_links")]
    links: Option<Links>,
    jetpack_sharing_enabled: Option<bool>,
    jetpack_shortlink: Option<String>,
    jetpack_videopress: Option<JetpackVideoPress>,
    jetpack_videopress_guid: Option<String>,
}

impl TryFrom<MediaWire> for Media {
    type Error = &'static str;

    fn try_from(wire: MediaWire) -> Result<Self, Self::Error> {
        let jetpack = match (
            wire.jetpack_sharing_enabled,
            wire.jetpack_shortlink,
            wire.jetpack_videopress,
            wire.jetpack_videopress_guid,
        ) {
            (None, None, None, None) => None,
            (Some(sharing_enabled), Some(shortlink), Some(videopress), Some(videopress_guid)) => {
                Some(JetpackMedia {
                    sharing_enabled,
                    shortlink,
                    videopress,
                    videopress_guid,
                })
            }
            _ => return Err("Jetpack media fields must be all present or all absent"),
        };

        Ok(Self {
            id: wire.id,
            publication: wire.publication,
            modification: wire.modification,
            slug: wire.slug,
            status: wire.status,
            kind: wire.kind,
            link: wire.link,
            title: wire.title,
            guid: wire.guid,
            author: wire.author,
            featured_media: wire.featured_media,
            comment_status: wire.comment_status,
            ping_status: wire.ping_status,
            template: wire.template,
            meta: wire.meta,
            class_list: wire.class_list,
            acf: wire.acf,
            description: wire.description,
            caption: wire.caption,
            alt_text: wire.alt_text,
            media_type: wire.media_type,
            mime_type: wire.mime_type,
            media_details: wire.media_details,
            post: wire.post,
            source_url: wire.source_url,
            filename: wire.filename,
            filesize: wire.filesize,
            links: wire.links,
            jetpack,
        })
    }
}

impl<'de> Deserialize<'de> for Media {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MediaWire::deserialize(deserializer)?
            .try_into()
            .map_err(D::Error::custom)
    }
}

impl Media {
    /// The publication time, from the API's UTC field.
    #[must_use]
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.publication.timestamp()
    }
}
