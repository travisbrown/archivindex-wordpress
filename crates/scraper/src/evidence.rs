//! Capture relationships observed in a WordPress archive, without lint or resume policy.
//!
//! Requests open groups in source order. Responses link to requests already seen, metadata
//! links to an accepted response already seen, and revisits resolve only earlier responses.
//! Forward references remain unlinked; this is the ordering supported by both consumers.
//! Duplicate request IDs select the latest request; the first response of a group wins.
//! Metadata occurrences are retained in order so consumers can choose their own policy.

use std::collections::HashMap;

use archivindex_warc::record::header::RevisitProfile;
use archivindex_warc::record::{FieldsBlock, Record, http};

#[derive(Debug, Eq, PartialEq)]
pub struct CaptureGroup {
    pub url: String,
    pub response: Option<StoredResponse>,
    pub metadata: Vec<StoredMetadata>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct StoredResponse {
    pub url: String,
    pub body: Vec<u8>,
    pub truncation: Option<String>,
    pub revisit: Option<StoredRevisit>,
}

impl StoredResponse {
    /// Interpret the HTTP head without decoding the entity body.
    pub fn metadata(&self) -> Option<http::ResponseMetadata> {
        http::ResponseMetadata::parse(&self.body)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct StoredRevisit {
    pub profile: StoredRevisitProfile,
    pub original: Option<usize>,
    pub identified_json: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredRevisitProfile {
    IdenticalPayloadDigest,
    ServerNotModified,
    Other,
}

#[derive(Debug, Eq, PartialEq)]
pub struct StoredMetadata {
    pub url: Option<String>,
    pub via: Option<String>,
    pub fields: bool,
}

/// Relationship problems carry evidence; callers choose severity and presentation.
#[derive(Debug, Eq, PartialEq)]
pub enum Problem {
    DuplicateRequestId(String),
    UnlinkedResponse(String),
    DuplicateResponse(String),
    UnlinkedMetadata(String),
    DuplicateMetadata(String),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateRequestId(id) => write!(f, "duplicate request record ID {id}"),
            Self::UnlinkedResponse(id) => {
                write!(f, "response or revisit record {id} has no linked request")
            }
            Self::DuplicateResponse(url) => write!(f, "capture of {url} has duplicate responses"),
            Self::UnlinkedMetadata(id) => {
                write!(f, "metadata record {id} has no linked response or revisit")
            }
            Self::DuplicateMetadata(url) => write!(f, "capture of {url} has duplicate metadata"),
        }
    }
}

#[derive(Default)]
pub struct Evidence {
    pub groups: Vec<CaptureGroup>,
    requests: HashMap<String, usize>,
    responses: HashMap<String, usize>,
}

impl Evidence {
    /// Retain only the response body and relationship fields needed after lint's borrowed visit.
    pub fn observe(&mut self, record: &Record) -> Option<Problem> {
        self.observe_with(record, <[u8]>::to_vec)
    }

    /// Resume owns its records, so transfer response bodies without copying them.
    pub fn observe_owned(&mut self, mut record: Record) -> Option<Problem> {
        let body = match &mut record {
            Record::Response { body, .. } | Record::Revisit { body, .. } => std::mem::take(body),
            _ => Vec::new(),
        };
        self.observe_with(&record, |_| body)
    }

    fn observe_with(
        &mut self,
        record: &Record,
        body: impl FnOnce(&[u8]) -> Vec<u8>,
    ) -> Option<Problem> {
        let id = &record.core().record_id;
        match record {
            Record::Request { header, .. } => {
                let duplicate = self
                    .requests
                    .insert(id.to_string(), self.groups.len())
                    .is_some();
                self.groups.push(CaptureGroup {
                    url: header.target_uri.to_string(),
                    response: None,
                    metadata: Vec::new(),
                });
                duplicate.then(|| Problem::DuplicateRequestId(id.to_string()))
            }
            Record::Response {
                header,
                body: bytes,
            } => {
                let request = header
                    .concurrent_to
                    .iter()
                    .find_map(|id| self.requests.get(id.as_str()).copied());
                self.attach_response(id.as_str(), request, || StoredResponse {
                    url: header.target_uri.to_string(),
                    body: body(bytes),
                    truncation: header
                        .core
                        .truncated
                        .as_ref()
                        .map(|reason| reason.as_str().to_owned()),
                    revisit: None,
                })
            }
            Record::Revisit {
                header,
                body: bytes,
            } => {
                let request = header
                    .concurrent_to
                    .iter()
                    .find_map(|id| self.requests.get(id.as_str()).copied());
                let original = header
                    .refers_to
                    .as_ref()
                    .and_then(|id| self.responses.get(id.as_str()).copied());
                let profile = match header.profile {
                    RevisitProfile::IdenticalPayloadDigest(_) => {
                        StoredRevisitProfile::IdenticalPayloadDigest
                    }
                    RevisitProfile::ServerNotModified(_) => StoredRevisitProfile::ServerNotModified,
                    RevisitProfile::Other(_) => StoredRevisitProfile::Other,
                };
                self.attach_response(id.as_str(), request, || StoredResponse {
                    url: header.target_uri.to_string(),
                    body: body(bytes),
                    truncation: header
                        .core
                        .truncated
                        .as_ref()
                        .map(|reason| reason.as_str().to_owned()),
                    revisit: Some(StoredRevisit {
                        profile,
                        original,
                        identified_json: header
                            .payload
                            .identified_payload_type
                            .as_ref()
                            .is_some_and(|media_type| media_type.is("application", "json")),
                    }),
                })
            }
            Record::Metadata { header, body } => {
                let Some(group) = header
                    .concurrent_to
                    .iter()
                    .find_map(|id| self.responses.get(id.as_str()).copied())
                else {
                    return Some(Problem::UnlinkedMetadata(id.to_string()));
                };
                let group = &mut self.groups[group];
                let duplicate = !group.metadata.is_empty();
                let (via, fields) = match body {
                    FieldsBlock::Fields(fields) => (fields.via().map(str::to_owned), true),
                    FieldsBlock::Raw(_) => (None, false),
                };
                group.metadata.push(StoredMetadata {
                    url: header.target_uri.as_ref().map(ToString::to_string),
                    via,
                    fields,
                });
                duplicate.then(|| Problem::DuplicateMetadata(group.url.clone()))
            }
            Record::Warcinfo { .. }
            | Record::Resource { .. }
            | Record::Conversion { .. }
            | Record::Continuation { .. }
            | Record::Other { .. } => None,
        }
    }

    fn attach_response(
        &mut self,
        id: &str,
        request: Option<usize>,
        response: impl FnOnce() -> StoredResponse,
    ) -> Option<Problem> {
        let Some(index) = request else {
            return Some(Problem::UnlinkedResponse(id.to_owned()));
        };
        let group = &mut self.groups[index];
        if group.response.is_some() {
            return Some(Problem::DuplicateResponse(group.url.clone()));
        }
        self.responses.insert(id.to_owned(), index);
        group.response = Some(response());
        None
    }
}

#[cfg(test)]
mod tests {
    use archivindex_warc::io::read::WarcReader;

    use super::{Evidence, Problem, Record, StoredRevisitProfile};

    const URL: &str = "https://example.com/wp-json";
    const RESPONSE: &str =
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}";

    fn record(kind: &str, id: &str, fields: &str, body: &str) -> String {
        format!(
            "WARC/1.1\r\nWARC-Type: {kind}\r\nWARC-Record-ID: <urn:test:{id}>\r\n\
             WARC-Date: 2026-08-20T00:00:00Z\r\nWARC-Target-URI: {URL}\r\n\
             Content-Length: {}\r\n{fields}\r\n{body}\r\n\r\n",
            body.len(),
        )
    }

    fn request(id: &str) -> String {
        record(
            "request",
            id,
            "Content-Type: application/http; msgtype=request\r\n",
            "GET /wp-json HTTP/1.1\r\nHost: example.com\r\n\r\n",
        )
    }

    fn response(id: &str, request: &str, fields: &str) -> String {
        record(
            "response",
            id,
            &format!("WARC-Concurrent-To: <urn:test:{request}>\r\n{fields}"),
            RESPONSE,
        )
    }

    fn metadata(id: &str, response: &str, via: &str) -> String {
        record(
            "metadata",
            id,
            &format!(
                "WARC-Concurrent-To: <urn:test:{response}>\r\nContent-Type: application/warc-fields\r\n"
            ),
            &format!("via: {via}\r\n"),
        )
    }

    fn parse(archive: &str) -> Vec<Record> {
        WarcReader::new(archive.as_bytes())
            .iter_records()
            .records()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn observe_both(archive: &str) -> (Evidence, Vec<Problem>) {
        let mut borrowed = Evidence::default();
        let mut owned = Evidence::default();
        let mut problems = Vec::new();
        for record in parse(archive) {
            let problem = borrowed.observe(&record);
            assert_eq!(problem, owned.observe_owned(record));
            problems.extend(problem);
        }
        assert_eq!(borrowed.groups, owned.groups);
        (owned, problems)
    }

    #[test]
    fn preserves_metadata_occurrences_and_first_response() {
        let archive = [
            request("request"),
            response("response", "request", "WARC-Truncated: time\r\n"),
            metadata("first", "response", "https://example.com/first"),
            metadata("last", "response", "https://example.com/last"),
            response("duplicate", "request", ""),
            metadata("unlinked", "duplicate", URL),
            record("resource", "unrelated", "", "unrelated"),
        ]
        .concat();
        let (evidence, problems) = observe_both(&archive);
        assert_eq!(evidence.groups.len(), 1);
        assert_eq!(
            problems,
            [
                Problem::DuplicateMetadata(URL.to_owned()),
                Problem::DuplicateResponse(URL.to_owned()),
                Problem::UnlinkedMetadata("urn:test:unlinked".to_owned()),
            ]
        );
        let group = &evidence.groups[0];
        assert_eq!(group.metadata.len(), 2);
        assert_eq!(
            group.metadata[0].via.as_deref(),
            Some("https://example.com/first")
        );
        assert_eq!(
            group.metadata[1].via.as_deref(),
            Some("https://example.com/last")
        );
        let response = group.response.as_ref().unwrap();
        assert_eq!(response.truncation.as_deref(), Some("time"));
        assert_eq!(response.metadata().unwrap().status, 200);
        assert_eq!(response.body, RESPONSE.as_bytes());
    }

    #[test]
    fn only_backward_links_resolve_and_latest_duplicate_request_wins() {
        let archive = [
            response("early", "request", ""),
            metadata("early-metadata", "response", URL),
            request("request"),
            request("request"),
            response("response", "request", ""),
        ]
        .concat();
        let (evidence, problems) = observe_both(&archive);
        assert_eq!(
            problems,
            [
                Problem::UnlinkedResponse("urn:test:early".to_owned()),
                Problem::UnlinkedMetadata("urn:test:early-metadata".to_owned()),
                Problem::DuplicateRequestId("urn:test:request".to_owned()),
            ]
        );
        assert!(evidence.groups[0].response.is_none());
        assert!(evidence.groups[1].response.is_some());
    }

    #[test]
    fn revisits_retain_profile_original_and_identified_payload_evidence() {
        let revisit = |id, request, original| {
            record(
                "revisit",
                id,
                &format!(
                    "WARC-Concurrent-To: <urn:test:{request}>\r\n\
             WARC-Refers-To: <urn:test:{original}>\r\n\
             WARC-Profile: http://netpreserve.org/warc/1.1/revisit/identical-payload-digest\r\n\
             WARC-Payload-Digest: sha1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\r\n\
             WARC-Identified-Payload-Type: application/json\r\n"
                ),
                "",
            )
        };
        let archive = [
            request("original-request"),
            response("original", "original-request", ""),
            request("revisit-request"),
            revisit("revisit", "revisit-request", "original"),
            request("unresolved-request"),
            revisit("unresolved", "unresolved-request", "absent"),
        ]
        .concat();
        let (evidence, problems) = observe_both(&archive);
        assert!(problems.is_empty());
        for (index, original) in [(1, Some(0)), (2, None)] {
            let revisit = evidence.groups[index]
                .response
                .as_ref()
                .unwrap()
                .revisit
                .as_ref()
                .unwrap();
            assert_eq!(
                revisit.profile,
                StoredRevisitProfile::IdenticalPayloadDigest
            );
            assert_eq!(revisit.original, original);
            assert!(revisit.identified_json);
        }
    }

    #[test]
    fn resume_and_lint_report_the_same_relationship_problem() {
        let archive = [
            request("request"),
            response("response", "request", ""),
            record("metadata", "first", "WARC-Concurrent-To: <urn:test:response>\r\nContent-Type: application/warc-fields\r\n", "title: Root\r\n"),
            metadata("last", "response", "https://example.com/last"),
        ]
        .concat();
        let custom = |record: String| {
            record.replace(
                &format!("WARC-Target-URI: {URL}"),
                &format!("WARC-Target-URI: {URL}/wp/v2/widgets"),
            )
        };
        let archive = archive
            + &[
                custom(request("custom-request")),
                custom(response("custom-response", "custom-request", "")),
                custom(metadata(
                    "custom-first",
                    "custom-response",
                    "https://example.com/wp-json/wp/v2/types",
                )),
                custom(metadata(
                    "custom-last",
                    "custom-response",
                    "https://example.com/wp-json/wp/v2/taxonomies",
                )),
                custom(record(
                    "metadata",
                    "custom-raw",
                    "WARC-Concurrent-To: <urn:test:custom-response>\r\n",
                    "raw metadata",
                )),
            ]
            .concat();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("archive.warc");
        std::fs::write(&path, archive).unwrap();
        let resume = crate::resume::inspect_archive(&path).unwrap();
        let lint = crate::lint::lint_archive(&path).unwrap();
        // Resume uses the last typed metadata via (ignoring a later raw block),
        // while lint uses the first metadata for the root's no-via requirement.
        assert!(
            resume
                .endpoints
                .contains(&crate::endpoint::Collection::Custom {
                    name: "widgets".to_owned(),
                    registry: crate::endpoint::Registry::Taxonomies,
                })
        );
        assert!(
            !lint
                .findings
                .iter()
                .any(|finding| finding.violation.rule() == "wrong_via")
        );

        assert!(
            resume
                .warnings
                .iter()
                .any(|warning| warning == &format!("capture of {URL} has duplicate metadata"))
        );
        assert!(
            lint.findings
                .iter()
                .any(|finding| finding.violation.rule() == "duplicate_capture_metadata")
        );
    }
}
