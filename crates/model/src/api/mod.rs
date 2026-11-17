//! Models for the `WordPress` REST API.

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::Offset;
use serde::{Deserialize, Serialize};

pub mod discovery;
pub mod extensions;
pub mod link;
pub mod media;
pub mod resource;

/// A `{ "rendered": … }` value, as returned for titles, content, and excerpts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rendered {
    pub rendered: String,
    pub protected: Option<bool>,
}

/// Optional local and UTC publication dates for content resources.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationDates {
    pub date: Option<DateTime>,
    pub date_gmt: Option<DateTime>,
}

impl PublicationDates {
    /// The publication time, from the UTC field.
    ///
    /// Returns `None` if the field is absent or outside the supported timestamp range.
    #[must_use]
    pub fn timestamp(&self) -> Option<Timestamp> {
        self.date_gmt
            .and_then(|value| Offset::UTC.to_timestamp(value).ok())
    }
}

/// Optional local and UTC modification dates for mutable resources.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModificationDates {
    pub modified: Option<DateTime>,
    pub modified_gmt: Option<DateTime>,
}

/// A GMT offset represented as a JSON number or string.
///
/// String values are retained without checking that they are numeric.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum GmtOffset {
    Number(f64),
    String(String),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Value, json};

    use super::discovery::{ApiRoot, Authentication, ErrorResponse, Namespace, Taxonomy, Type};
    use super::media::{Media, MediaType};
    use super::resource::{DiscussionStatus, Post, PostFormat};
    use super::{ModificationDates, PublicationDates};

    #[test]
    fn resource_dates_preserve_civil_times_and_use_the_gmt_field()
    -> Result<(), Box<dyn std::error::Error>> {
        let publication = json!({
            "date": "2026-08-28T23:57:06.123456789",
            "date_gmt": "2026-08-28T21:57:06.123456789"
        });
        let dates: PublicationDates = serde_json::from_value(publication.clone())?;
        assert_eq!(serde_json::to_value(dates)?, publication);
        assert_eq!(
            dates.timestamp(),
            Some("2026-08-28T21:57:06.123456789Z".parse()?)
        );

        let modification = json!({
            "modified": "2026-08-29T00:15:00",
            "modified_gmt": "2026-08-28T22:15:00"
        });
        let dates: ModificationDates = serde_json::from_value(modification.clone())?;
        assert_eq!(serde_json::to_value(dates)?, modification);
        Ok(())
    }

    #[test]
    fn publication_times_require_a_representable_gmt_field()
    -> Result<(), Box<dyn std::error::Error>> {
        for value in [
            json!({}),
            json!({"date": "2026-08-28T23:57:06", "date_gmt": null}),
            json!({"date_gmt": "9999-12-31T23:59:59"}),
        ] {
            let dates: PublicationDates = serde_json::from_value(value)?;
            assert_eq!(dates.timestamp(), None);
        }
        assert!(
            serde_json::from_value::<PublicationDates>(json!({
                "date_gmt": "2026-02-30T12:00:00"
            }))
            .is_err()
        );
        Ok(())
    }

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
    fn empty_authentication_variant_roundtrips_without_accepting_elements()
    -> Result<(), serde_json::Error> {
        let authentication = serde_json::from_str::<Authentication>("[]")?;
        assert_eq!(authentication, Authentication::Empty([]));
        assert_eq!(serde_json::to_string(&authentication)?, "[]");

        for value in ["[null]", "[false]", "[1]", r#"["method"]"#, "[{}]", "[[]]"] {
            assert!(
                !matches!(
                    serde_json::from_str::<Authentication>(value),
                    Ok(Authentication::Empty(_))
                ),
                "{value}"
            );
        }
        Ok(())
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
        let Authentication::Methods(authentication) = root.authentication else {
            panic!("expected authentication methods");
        };
        assert_eq!(
            authentication
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
    fn legacy_type_registry_entries_can_omit_the_implied_namespace() {
        let types = serde_json::from_value::<BTreeMap<String, Type>>(json!({
            "post": {
                "name": "Posts",
                "description": "",
                "hierarchical": false,
                "slug": "post",
                "taxonomies": ["category", "post_tag"],
                "rest_base": "posts",
                "_links": {
                    "collection": [{"href": "https://example.com/wp-json/wp/v2/types"}],
                    "wp:items": [{"href": "https://example.com/wp-json/wp/v2/posts"}],
                    "curies": [{
                        "name": "wp",
                        "href": "https://api.w.org/{rel}",
                        "templated": true
                    }]
                }
            }
        }))
        .expect("a legacy type registry");

        assert_eq!(types["post"].rest_namespace, None);
        assert_eq!(
            (&types["post"].yoast.head, &types["post"].yoast.head_json),
            (&None, &None)
        );

        let with_redacted_yoast = serde_json::from_value::<Type>(json!({
            "name": "Pages",
            "description": "",
            "hierarchical": true,
            "slug": "page",
            "taxonomies": [],
            "rest_base": "pages",
            "yoast_head": null,
            "yoast_head_json": null,
            "_links": {}
        }))
        .expect("a type with redacted Yoast metadata");
        let yoast = with_redacted_yoast.yoast;
        assert_eq!((yoast.head, yoast.head_json), (None, None));
    }

    fn media_attachment() -> Value {
        json!({
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
        })
    }

    #[test]
    fn media_attachments_parse() {
        let media =
            serde_json::from_value::<Media>(media_attachment()).expect("a media attachment");

        assert_eq!(
            media.timestamp().expect("a timestamp").to_string(),
            "2026-08-28T21:57:06Z"
        );
        assert_eq!(media.jetpack, None);
    }

    #[test]
    fn partial_jetpack_media_metadata_is_rejected() {
        let mut complete = media_attachment();
        let object = complete.as_object_mut().expect("a media object");
        object.insert("jetpack_sharing_enabled".to_owned(), json!(true));
        object.insert(
            "jetpack_shortlink".to_owned(),
            json!("https://wp.me/example"),
        );
        object.insert(
            "jetpack_videopress".to_owned(),
            json!({
                "title": "Example video",
                "description": "",
                "caption": "",
                "guid": null,
                "rating": null,
                "allow_download": 0,
                "display_embed": 0,
                "privacy_setting": 2,
                "needs_playback_token": false,
                "is_private": false,
                "private_enabled_for_site": false
            }),
        );
        object.insert("jetpack_videopress_guid".to_owned(), json!("video-guid"));

        let media = serde_json::from_value::<Media>(complete.clone()).expect("Jetpack media");
        assert_eq!(
            media.jetpack.expect("Jetpack metadata").videopress_guid,
            "video-guid"
        );

        complete
            .as_object_mut()
            .expect("a media object")
            .remove("jetpack_videopress_guid");
        assert!(serde_json::from_value::<Media>(complete).is_err());
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
