//! Structural validation of collection archives produced by [`ArchiveDriver`](crate::archive::ArchiveDriver).

use std::collections::{BTreeMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};

use archivindex_warc::io::read::WarcReader;
use archivindex_warc::record::{Record, http, payload};
use archivindex_warc_ops::lint::{Custom, Finding, Findings, Linter, Rule, Severity, Violation};
use serde_json::Value;
use url::Url;

use crate::archive::{Site, SiteError};
use crate::endpoint::{Collection, Endpoint, EndpointType, ROOT_ENDPOINTS, Registry};
use crate::evidence::{
    CaptureGroup, Evidence, Problem, StoredResponse, StoredRevisit, StoredRevisitProfile,
};

/// Counts advertised for one successfully probed and paginated endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginationSummary {
    /// Bare collection endpoint URI.
    pub endpoint: String,
    /// Greatest page count advertised during pagination.
    pub pages: Option<usize>,
    /// Item count advertised by the endpoint probe.
    pub items: Option<usize>,
}

/// Results of linting one `WordPress` collection archive.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LintReport {
    /// The findings of the pass: those of the standard rules and those of the rules below, in
    /// the order the pass reported them.
    pub findings: Vec<Finding>,
    /// Number of records that passed all core WARC lint rules.
    pub core_lints_passed: usize,
    /// Successfully probed endpoints in probe order, with their advertised counts.
    pub pagination: Vec<PaginationSummary>,
    /// Number of required root captures found.
    pub roots: usize,
    /// Number of required known endpoint probes found.
    pub known_probes: usize,
    /// Number of registry-advertised custom endpoint probes found.
    pub custom_probes: usize,
}

impl LintReport {
    /// Whether the archive has no errors or warnings.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// Number of error findings.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.violation.severity() == Severity::Error)
            .count()
    }

    /// Number of warning findings.
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.violation.severity() == Severity::Warning)
            .count()
    }
}

/// A collection archive could not be read or did not identify a `WordPress` site.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The input file could not be opened or read.
    #[error("cannot read {}: {source}", path.display())]
    Io {
        /// Path that was being read.
        path: PathBuf,
        /// Underlying file error.
        #[source]
        source: std::io::Error,
    },
    /// A WARC record is syntactically or semantically invalid.
    #[error("cannot read a WARC record from {}: {source}", path.display())]
    Warc {
        /// Path that was being read.
        path: PathBuf,
        /// Underlying WARC error.
        #[source]
        source: archivindex_warc::io::read::Error,
    },
    /// No request identifies a `WordPress` installation.
    #[error("{} contains no WordPress REST API requests", .0.display())]
    NoWordPressRequests(PathBuf),
    /// The inferred installation URL is invalid.
    #[error("cannot infer a WordPress site from {url:?}: {source}")]
    Site {
        /// Request URI used for inference.
        url: String,
        /// Why the inferred site was invalid.
        #[source]
        source: SiteError,
    },
}

/// Lint a plain or gzip-compressed `WordPress` collection WARC.
///
/// One pass checks the file against the rules of [`archivindex_warc_ops::lint`] and the
/// `WordPress` rules below. The archive must begin with all API roots and known probes, followed
/// by registry-advertised custom probes. Every successful probe must have one correctly linked
/// pagination traversal.
///
/// # Errors
///
/// Returns [`Error`] when the file cannot be read as a WARC or no `WordPress` request identifies the
/// installation. Protocol violations are returned as findings rather than errors.
pub fn lint_archive(path: impl AsRef<Path>) -> Result<LintReport, Error> {
    let path = path.as_ref();
    let gzip = crate::read::is_gzip_file(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    if gzip {
        let reader = WarcReader::from_path_gzip(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        lint_reader(reader, path)
    } else {
        let reader = WarcReader::from_path(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        lint_reader(reader, path)
    }
}

/// Run the standard rules and the `WordPress` rules over `reader` in one pass.
fn lint_reader<R: BufRead>(reader: WarcReader<R>, path: &Path) -> Result<LintReport, Error> {
    let mut rule = WordPressRules::new(path);
    let mut findings = Vec::new();
    let mut linter = Linter::new(reader).with_rule(&mut rule);

    for checked in linter.by_ref() {
        let checked = checked.map_err(|source| Error::Warc {
            path: path.to_owned(),
            source,
        })?;
        if let Err(finding) = checked {
            findings.push(*finding);
        }
    }

    let records = linter.position();
    drop(linter);

    if let Some(error) = rule.error {
        return Err(error);
    }

    let failed_core_records = findings
        .iter()
        .filter(|finding| !matches!(&finding.violation, Violation::Custom(_)))
        .filter_map(|finding| finding.subject.as_ref().map(|subject| subject.index))
        .collect::<HashSet<_>>()
        .len();

    Ok(LintReport {
        findings,
        core_lints_passed: records - failed_core_records,
        pagination: rule.analysis.pagination,
        roots: rule.analysis.roots,
        known_probes: rule.analysis.known_probes,
        custom_probes: rule.analysis.custom_probes,
    })
}

/// What the whole-file analysis gathers: its findings, and the summary the report carries.
///
/// The findings are held until the pass settles the end of the file, since the analysis reads
/// captures the whole file has to have been read to relate.
#[derive(Debug, Default)]
struct Analysis {
    findings: Vec<Custom>,
    pagination: Vec<PaginationSummary>,
    roots: usize,
    known_probes: usize,
    custom_probes: usize,
}

/// How an HTTP pagination link identifies the expected page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PageUriMatch {
    Different,
    Exact,
    /// The URL differs only by an additional `attest=true` query parameter.
    Attested,
}

const ATTEST_LINK_WARNING: &str =
    "pagination Link targets add attest=true; treating it as an advisory query parameter";

#[derive(Clone, Debug)]
struct Probe {
    collection: Collection,
    url: String,
    status: Option<u16>,
    items: Option<usize>,
}

#[derive(Clone, Debug)]
struct PageCapture {
    group: usize,
    page: Option<usize>,
    valid_query: bool,
}

/// Relate the captures the file holds, once every record has been read.
fn analyse(groups: &[CaptureGroup], path: &Path, report: &mut Analysis) -> Result<(), Error> {
    let site = groups
        .iter()
        .find_map(|group| site_from_request(&group.url))
        .transpose()?
        .ok_or_else(|| Error::NoWordPressRequests(path.to_owned()))?;
    let mut checked_shapes = HashSet::new();

    let customs = discover_customs(groups, &site, report);
    let expected_initial = expected_initial(&site, &customs);

    let mut previous = None;
    let mut probes = Vec::new();
    for (position, (url, collection)) in expected_initial.iter().enumerate() {
        let candidates = initial_capture_candidates(groups, url);
        let group = candidates.first().copied();
        let label = collection.as_ref().map_or_else(
            || format!("initial root {url}"),
            |collection| format!("{} probe", collection.name()),
        );
        if candidates.is_empty() {
            error(
                report,
                "missing_required_capture",
                format!("missing required {label}"),
            );
        } else {
            if candidates.len() > 1 {
                error(
                    report,
                    "repeated_initial_capture",
                    format!("{label} is captured {} times", candidates.len()),
                );
            }
            if let Some(previous) = previous
                && group.is_some_and(|group| group < previous)
            {
                error(
                    report,
                    "initial_capture_out_of_order",
                    format!("{label} is out of initial capture order"),
                );
            }
            if let Some(group) = group {
                previous = Some(group);
                check_capture_shape(groups, group, &mut checked_shapes, report);
                let expected_via = collection
                    .as_ref()
                    .and_then(Collection::registry)
                    .map(|registry| format!("{}{}", site.root(), registry.path()));
                check_via(&groups[group], expected_via.as_deref(), &label, report);
            }
            if position < ROOT_ENDPOINTS.len() {
                report.roots += 1;
            } else if position < ROOT_ENDPOINTS.len() + Endpoint::ALL.len() {
                report.known_probes += 1;
            } else {
                report.custom_probes += 1;
            }
        }

        if let Some(collection) = collection {
            let response = group.and_then(|group| response_metadata(&groups[group]));
            let status = response.as_ref().map(|metadata| metadata.status);
            let items = response
                .as_ref()
                .and_then(|metadata| numeric_header(metadata, "x-wp-total"));
            if status.is_some_and(is_success) && items.is_none() {
                error(
                    report,
                    "missing_total_items",
                    format!(
                        "successful {} probe has missing or invalid X-WP-Total",
                        collection.name()
                    ),
                );
            }
            if let (Some(group), Some(metadata)) = (group, response.as_ref())
                && is_success(metadata.status)
            {
                check_probe_response(groups, group, metadata, collection.name(), report);
            }
            probes.push(Probe {
                collection: collection.clone(),
                url: url.clone(),
                status,
                items,
            });
        }
    }

    check_unadvertised_custom_probes(groups, &site, &customs, report);
    lint_pagination(groups, &probes, &mut checked_shapes, report);
    Ok(())
}

fn expected_initial(site: &Site, customs: &[Collection]) -> Vec<(String, Option<Collection>)> {
    let mut expected = ROOT_ENDPOINTS
        .map(|root| (format!("{}{root}", site.root()), None))
        .to_vec();
    expected.extend(Endpoint::ALL.map(|endpoint| {
        (
            endpoint_url(site, endpoint.name()),
            Some(Collection::Known(endpoint)),
        )
    }));
    expected.extend(
        customs
            .iter()
            .cloned()
            .map(|collection| (endpoint_url(site, collection.name()), Some(collection))),
    );
    expected
}

fn initial_capture_candidates(groups: &[CaptureGroup], url: &str) -> Vec<usize> {
    groups
        .iter()
        .enumerate()
        .filter(|(_, group)| group.url == url && !is_server_challenge(group))
        .map(|(index, _)| index)
        .collect()
}

/// The rules a `WordPress` collection archive is held to, checked beside the standard rules.
///
/// The rules relate captures that only the whole file shows, so a pass collects the records as
/// they go by and reports what it finds once the file ends. A record the pass cannot read is
/// checked against no rule, and the capture it belonged to is reported incomplete.
pub struct WordPressRules {
    /// The file being read, named by the errors that end the pass.
    path: PathBuf,
    /// Shared request/response/metadata evidence in source order.
    evidence: Evidence,
    /// What the whole-file analysis found.
    analysis: Analysis,
    /// Why the archive could not be analysed, if it could not be.
    error: Option<Error>,
}

impl WordPressRules {
    /// Hold the archive at `path` to the `WordPress` capture and pagination rules.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            evidence: Evidence::default(),
            analysis: Analysis::default(),
            error: None,
        }
    }
}

impl Rule for WordPressRules {
    fn check(&mut self, index: usize, record: &Record, findings: &mut Findings<'_>) {
        if let Some(problem) = self.evidence.observe(record) {
            let rule = match &problem {
                Problem::DuplicateRequestId(_) => "duplicate_request_record_id",
                Problem::UnlinkedResponse(_) => "unlinked_response_record",
                Problem::DuplicateResponse(_) => "duplicate_capture_response",
                Problem::UnlinkedMetadata(_) => "unlinked_metadata_record",
                Problem::DuplicateMetadata(_) => "duplicate_capture_metadata",
            };
            findings.fault(
                index,
                &record.core().record_id,
                Custom::error(rule, problem.to_string()),
            );
        }
    }

    fn finish(&mut self, findings: &mut Findings<'_>) {
        if let Err(error) = analyse(&self.evidence.groups, &self.path, &mut self.analysis) {
            self.error = Some(error);
        }
        for violation in self.analysis.findings.drain(..) {
            findings.fault_file(violation);
        }
    }
}

fn site_from_request(request: &str) -> Option<Result<Site, Error>> {
    let mut url = Url::parse(request).ok()?;
    let marker = url.path().find("/wp-json")?;
    let path = match &url.path()[..marker] {
        "" => "/".to_owned(),
        path => path.to_owned(),
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    let inferred = url.as_str().trim_end_matches('/');
    Some(Site::parse(inferred).map_err(|source| Error::Site {
        url: request.to_owned(),
        source,
    }))
}

fn discover_customs(
    groups: &[CaptureGroup],
    site: &Site,
    report: &mut Analysis,
) -> Vec<Collection> {
    let mut custom = Vec::new();
    for registry in Registry::ALL {
        let url = format!("{}{}", site.root(), registry.path());
        let Some(group) = groups
            .iter()
            .find(|group| group.url == url && !is_server_challenge(group))
        else {
            continue;
        };
        let Some(response) = &group.response else {
            continue;
        };
        let Some(metadata) = response.metadata() else {
            continue;
        };
        if !(200..300).contains(&metadata.status) {
            continue;
        }
        let entity = match payload::entity_body(&response.body) {
            Ok(entity) => entity,
            Err(source) => {
                error(
                    report,
                    "unreadable_registry_payload",
                    format!("cannot read {url} response payload: {source}"),
                );
                continue;
            }
        };
        let entries = match EndpointType::parse_registry(&entity) {
            Ok(entries) => entries,
            Err(source) => {
                error(
                    report,
                    "unreadable_registry_response",
                    format!("unreadable {url} registry response: {source}"),
                );
                continue;
            }
        };
        for name in EndpointType::custom_endpoints(&entries) {
            if !custom.iter().any(|item: &Collection| item.name() == name) {
                custom.push(Collection::Custom {
                    name: name.to_owned(),
                    registry,
                });
            }
        }
    }
    custom
}

fn check_unadvertised_custom_probes(
    groups: &[CaptureGroup],
    site: &Site,
    custom: &[Collection],
    report: &mut Analysis,
) {
    let known = Endpoint::ALL.map(Endpoint::name);
    let prefix = format!("{}wp-json/wp/v2/", site.root());
    for group in groups {
        let Ok(url) = Url::parse(&group.url) else {
            continue;
        };
        let Some(name) = url.path().strip_prefix(
            Url::parse(&prefix)
                .expect("a site root makes a valid endpoint prefix")
                .path(),
        ) else {
            continue;
        };
        if url.query().is_none()
            && !name.is_empty()
            && !name.contains('/')
            && !known.contains(&name)
            && !ROOT_ENDPOINTS
                .iter()
                .any(|root| root.strip_prefix("wp-json/wp/v2/") == Some(name))
            && !custom.iter().any(|collection| collection.name() == name)
        {
            error(
                report,
                "unadvertised_custom_probe",
                format!("custom endpoint probe {name:?} was not advertised by a registry"),
            );
        }
    }
}

fn check_capture_shape(
    groups: &[CaptureGroup],
    index: usize,
    checked: &mut HashSet<usize>,
    report: &mut Analysis,
) {
    if !checked.insert(index) {
        return;
    }
    let group = &groups[index];
    let Some(response) = &group.response else {
        error(
            report,
            "missing_capture_response",
            format!("capture of {} is missing a response or revisit", group.url),
        );
        return;
    };
    if response.url != group.url {
        error(
            report,
            "capture_uri_mismatch",
            format!(
                "capture request URI {} does not match response URI {}",
                group.url, response.url
            ),
        );
    }
    if let Some(reason) = response
        .truncation
        .as_deref()
        .filter(|reason| !intentional_revisit_truncation(response, reason))
    {
        error(
            report,
            "truncated_capture_response",
            format!(
                "capture of {} has a response truncated because of {reason}",
                group.url
            ),
        );
    }
    if response.metadata().is_none() {
        error(
            report,
            "invalid_http_response",
            format!("capture of {} has an invalid HTTP response", group.url),
        );
    }
    let Some(metadata) = group.metadata.first() else {
        error(
            report,
            "missing_capture_metadata",
            format!("capture of {} is missing metadata", group.url),
        );
        return;
    };
    if metadata.url.as_deref() != Some(response.url.as_str()) {
        error(
            report,
            "metadata_uri_mismatch",
            format!(
                "capture response URI {} does not match metadata URI {}",
                response.url,
                metadata.url.as_deref().unwrap_or("<missing>")
            ),
        );
    }
    if !metadata.fields {
        error(
            report,
            "metadata_not_warc_fields",
            format!(
                "capture of {} metadata is not application/warc-fields",
                group.url
            ),
        );
    }
}

fn check_via(group: &CaptureGroup, expected: Option<&str>, label: &str, report: &mut Analysis) {
    let actual = group
        .metadata
        .first()
        .and_then(|metadata| metadata.via.as_deref());
    if actual != expected {
        error(
            report,
            "wrong_via",
            format!(
                "{label} has via {}, expected {}",
                display_optional(actual),
                display_optional(expected)
            ),
        );
    }
}

fn lint_pagination(
    groups: &[CaptureGroup],
    probes: &[Probe],
    checked_shapes: &mut HashSet<usize>,
    report: &mut Analysis,
) {
    let (mut pages, encountered) = collect_page_captures(groups, probes, report);

    let successful = probes
        .iter()
        .filter(|probe| probe.status.is_some_and(is_success))
        .map(|probe| probe.collection.name().to_owned())
        .collect::<Vec<_>>();
    let encountered_unique = encountered
        .iter()
        .filter(|name| pages.contains_key(*name))
        .cloned()
        .collect::<Vec<_>>();
    if encountered_unique
        != successful
            .iter()
            .filter(|name| pages.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>()
    {
        error(
            report,
            "pagination_out_of_probe_order",
            format!(
                "pagination endpoints are out of probe order: found {}, expected {}",
                encountered_unique.join(", "),
                successful.join(", ")
            ),
        );
    }
    let mut seen = HashSet::new();
    for name in &encountered {
        if !seen.insert(name) {
            error(
                report,
                "interrupted_pagination_series",
                format!("pagination series for {name} is interrupted by another endpoint"),
            );
        }
    }

    for probe in probes {
        let name = probe.collection.name();
        let captures = pages.remove(name).unwrap_or_default();
        let success = probe.status.is_some_and(is_success);
        if !success {
            if !captures.is_empty() {
                error(
                    report,
                    "unexpected_pagination_series",
                    format!(
                        "unsuccessful {name} probe has a pagination series of {} captures",
                        captures.len()
                    ),
                );
            }
            continue;
        }
        if captures.is_empty() {
            error(
                report,
                "missing_pagination_series",
                format!("successful {name} probe has no pagination series"),
            );
            report.pagination.push(PaginationSummary {
                endpoint: probe.url.clone(),
                pages: None,
                items: probe.items,
            });
            continue;
        }
        let total_pages = lint_series(groups, probe, &captures, checked_shapes, report);
        report.pagination.push(PaginationSummary {
            endpoint: probe.url.clone(),
            pages: total_pages,
            items: probe.items,
        });
    }
}

fn collect_page_captures(
    groups: &[CaptureGroup],
    probes: &[Probe],
    report: &mut Analysis,
) -> (BTreeMap<String, Vec<PageCapture>>, Vec<String>) {
    let mut pages = BTreeMap::<String, Vec<PageCapture>>::new();
    let mut cutoff = None;
    let endpoints = probes
        .iter()
        .map(|probe| (probe.url.as_str(), probe.collection.name()))
        .collect::<Vec<_>>();
    let mut encountered = Vec::<String>::new();
    for (index, group) in groups.iter().enumerate() {
        let Some((_, name)) = endpoints
            .iter()
            .find(|(probe_url, _)| group.url.starts_with(&format!("{probe_url}?")))
        else {
            continue;
        };
        let (page, valid_query, before) = page_query(&group.url);
        if let Some(before) = before {
            match &cutoff {
                None => cutoff = Some(before),
                Some(expected) if expected != &before => error(
                    report,
                    "conflicting_before_cutoff",
                    format!(
                        "pagination request {} has a conflicting before cutoff",
                        group.url
                    ),
                ),
                Some(_) => {}
            }
        }
        if encountered.last().is_none_or(|previous| previous != *name) {
            encountered.push((*name).to_owned());
        }
        pages
            .entry((*name).to_owned())
            .or_default()
            .push(PageCapture {
                group: index,
                page,
                valid_query,
            });
    }
    (pages, encountered)
}

fn page_query(url: &str) -> (Option<usize>, bool, Option<String>) {
    let Ok(url) = Url::parse(url) else {
        return (None, false, None);
    };
    let mut values = BTreeMap::<String, Vec<String>>::new();
    for (name, value) in url.query_pairs() {
        values
            .entry(name.into_owned())
            .or_default()
            .push(value.into_owned());
    }
    let one = |name: &str| {
        values
            .get(name)
            .and_then(|values| (values.len() == 1).then(|| values[0].as_str()))
    };
    let page = one("page").and_then(|value| value.parse::<usize>().ok());
    let before = one("before").map(str::to_owned);
    let valid = values.len() == 5
        && page.is_some_and(|page| page > 0)
        && before
            .as_deref()
            .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok())
        && one("orderby") == Some("id")
        && one("order") == Some("asc")
        && one("per_page")
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|per_page| (1..=100).contains(&per_page))
        && url.fragment().is_none();
    (page, valid, before)
}

fn lint_series(
    groups: &[CaptureGroup],
    probe: &Probe,
    captures: &[PageCapture],
    checked_shapes: &mut HashSet<usize>,
    report: &mut Analysis,
) -> Option<usize> {
    let name = probe.collection.name();
    let successful = captures
        .iter()
        .filter(|capture| is_successful_page_capture(&groups[capture.group]))
        .cloned()
        .collect::<Vec<_>>();
    let split = successful
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, capture)| (capture.page == Some(1)).then_some(index));
    let (pagination, legacy_validation) = split.map_or((successful.as_slice(), &[][..]), |index| {
        successful.split_at(index)
    });
    let total_pages = pagination
        .iter()
        .filter_map(|capture| response_metadata(&groups[capture.group]))
        .filter_map(|metadata| numeric_header(&metadata, "x-wp-totalpages"))
        .max();

    lint_pass(
        groups,
        name,
        pagination,
        &probe.url,
        total_pages,
        "pagination",
        checked_shapes,
        report,
    );
    if !legacy_validation.is_empty() {
        if total_pages.is_some_and(|pages| pages > 1) {
            let via = pagination.last().map_or(probe.url.as_str(), |capture| {
                groups[capture.group].url.as_str()
            });
            lint_pass(
                groups,
                name,
                legacy_validation,
                via,
                total_pages,
                "legacy validation",
                checked_shapes,
                report,
            );
        } else {
            error(
                report,
                "unnecessary_second_pass",
                format!("{name} pagination series has an unnecessary second pass"),
            );
        }
    }
    total_pages
}

fn is_successful_page_capture(group: &CaptureGroup) -> bool {
    let Some(response) = &group.response else {
        return false;
    };
    if response
        .truncation
        .as_deref()
        .is_some_and(|reason| !intentional_revisit_truncation(response, reason))
    {
        return false;
    }
    response_metadata(group).is_some_and(|metadata| matches!(metadata.status, 200 | 304))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the state of one lint pass, and a config struct would be \
              built and destructured at the single call site"
)]
fn lint_pass(
    groups: &[CaptureGroup],
    name: &str,
    captures: &[PageCapture],
    first_via: &str,
    expected_total: Option<usize>,
    pass: &str,
    checked_shapes: &mut HashSet<usize>,
    report: &mut Analysis,
) {
    for (position, capture) in captures.iter().enumerate() {
        let expected_via = if position == 0 {
            first_via
        } else {
            &groups[captures[position - 1].group].url
        };
        lint_page_capture(
            groups,
            capture,
            position,
            expected_via,
            name,
            pass,
            checked_shapes,
            report,
        );
    }

    if let Some(total) = expected_total
        && captures.len() != total.max(1)
    {
        error(
            report,
            "wrong_pagination_length",
            format!(
                "{name} {pass} pass has {} captures, expected {} for {total} advertised pages",
                captures.len(),
                total.max(1)
            ),
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the arguments are the state of one lint pass, and a config struct would be \
              built and destructured at the single call site"
)]
fn lint_page_capture(
    groups: &[CaptureGroup],
    capture: &PageCapture,
    position: usize,
    expected_via: &str,
    name: &str,
    pass: &str,
    checked_shapes: &mut HashSet<usize>,
    report: &mut Analysis,
) {
    let group = &groups[capture.group];
    check_capture_shape(groups, capture.group, checked_shapes, report);
    if !capture.valid_query {
        error(
            report,
            "malformed_pagination_uri",
            format!(
                "{name} {pass} pagination URI has the wrong shape: {}",
                group.url
            ),
        );
    }
    let expected_page = position + 1;
    if capture.page != Some(expected_page) {
        error(
            report,
            "pagination_page_out_of_order",
            format!(
                "{name} {pass} pagination capture {} is page {}, expected page {expected_page}",
                position + 1,
                capture
                    .page
                    .map_or_else(|| "?".to_owned(), |page| page.to_string())
            ),
        );
    }
    check_via(
        group,
        Some(expected_via),
        &format!("{name} {pass} page {expected_page}"),
        report,
    );

    let Some(metadata) = response_metadata(group) else {
        return;
    };
    if !matches!(metadata.status, 200 | 304) {
        error(
            report,
            "unexpected_page_status",
            format!(
                "{name} {pass} page {expected_page} has unexpected HTTP status {}",
                metadata.status
            ),
        );
    }
    let advertised = numeric_header(&metadata, "x-wp-totalpages");
    if advertised.is_none() {
        error(
            report,
            "missing_total_pages",
            format!("{name} {pass} page {expected_page} has missing or invalid X-WP-TotalPages"),
        );
    }
    check_json_array(
        groups,
        capture.group,
        &metadata,
        name,
        pass,
        expected_page,
        report,
    );
    check_header_links(
        group,
        &metadata,
        expected_page,
        advertised,
        name,
        pass,
        report,
    );
}

fn check_json_array(
    groups: &[CaptureGroup],
    group: usize,
    metadata: &http::ResponseMetadata,
    name: &str,
    pass: &str,
    page: usize,
    report: &mut Analysis,
) {
    if metadata.status != 200 {
        return;
    }
    let valid = match resolved_payload(groups, group) {
        ResolvedPayload::Block(body) => payload::entity_body(body)
            .ok()
            .and_then(|entity| serde_json::from_slice::<Value>(&entity).ok())
            .is_some_and(|value| value.is_array()),
        ResolvedPayload::IdentifiedJson => true,
        ResolvedPayload::Missing => false,
    };
    if !valid {
        error(
            report,
            "page_not_json_array",
            format!("{name} {pass} page {page} response is not a JSON array"),
        );
    }
}

fn check_probe_response(
    groups: &[CaptureGroup],
    group: usize,
    metadata: &http::ResponseMetadata,
    name: &str,
    report: &mut Analysis,
) {
    if numeric_header(metadata, "x-wp-totalpages").is_none() {
        error(
            report,
            "missing_total_pages",
            format!("successful {name} probe has missing or invalid X-WP-TotalPages"),
        );
    }
    if metadata.status == 200 {
        let valid = match resolved_payload(groups, group) {
            ResolvedPayload::Block(body) => payload::entity_body(body)
                .ok()
                .and_then(|entity| serde_json::from_slice::<Value>(&entity).ok())
                .is_some_and(|value| value.is_array()),
            ResolvedPayload::IdentifiedJson => true,
            ResolvedPayload::Missing => false,
        };
        if !valid {
            error(
                report,
                "probe_not_json_array",
                format!("successful {name} probe response is not a JSON array"),
            );
        }
    }
}

const fn intentional_revisit_truncation(response: &StoredResponse, reason: &str) -> bool {
    reason.eq_ignore_ascii_case("length")
        && matches!(
            &response.revisit,
            Some(StoredRevisit {
                profile: StoredRevisitProfile::IdenticalPayloadDigest
                    | StoredRevisitProfile::ServerNotModified,
                ..
            })
        )
}

enum ResolvedPayload<'a> {
    Block(&'a [u8]),
    IdentifiedJson,
    Missing,
}

fn resolved_payload(groups: &[CaptureGroup], mut group: usize) -> ResolvedPayload<'_> {
    let mut visited = HashSet::new();
    while visited.insert(group) {
        let Some(response) = groups.get(group).and_then(|group| group.response.as_ref()) else {
            return ResolvedPayload::Missing;
        };
        let Some(revisit) = &response.revisit else {
            return ResolvedPayload::Block(&response.body);
        };
        if let Some(original) = revisit.original {
            group = original;
        } else if revisit.identified_json {
            return ResolvedPayload::IdentifiedJson;
        } else {
            return ResolvedPayload::Missing;
        }
    }
    ResolvedPayload::Missing
}

fn check_header_links(
    group: &CaptureGroup,
    metadata: &http::ResponseMetadata,
    page: usize,
    total: Option<usize>,
    name: &str,
    pass: &str,
    report: &mut Analysis,
) {
    let Some(total) = total else {
        return;
    };
    for (relation, expected_page) in [
        ("prev", (page > 1).then_some(page - 1)),
        ("next", (page < total).then_some(page + 1)),
    ] {
        let Some(expected_page) = expected_page else {
            continue;
        };
        let mut attested = false;
        let found = metadata
            .headers("link")
            .filter_map(|value| std::str::from_utf8(value).ok())
            .flat_map(parse_links)
            .any(|(target, relations)| {
                if !relations.iter().any(|value| value == relation) {
                    return false;
                }
                match page_uri_match(&target, &group.url, expected_page) {
                    PageUriMatch::Different => false,
                    PageUriMatch::Exact => true,
                    PageUriMatch::Attested => {
                        attested = true;
                        true
                    }
                }
            });
        if attested {
            warning_once(report, "attested_pagination_link", ATTEST_LINK_WARNING);
        }
        if !found {
            warning(
                report,
                "missing_pagination_link",
                format!(
                    "{name} {pass} page {page} has no expected {relation} Link to page {expected_page}"
                ),
            );
        }
    }
}

fn parse_links(value: &str) -> impl Iterator<Item = (String, Vec<String>)> + '_ {
    value.split(',').filter_map(|part| {
        let part = part.trim();
        let end = part.find('>')?;
        let target = part.get(1..end)?.to_owned();
        let relations = part[end + 1..]
            .split(';')
            .filter_map(|parameter| parameter.trim().split_once('='))
            .find_map(|(name, value)| {
                name.eq_ignore_ascii_case("rel").then(|| {
                    value
                        .trim_matches('"')
                        .split_ascii_whitespace()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();
        Some((target, relations))
    })
}

fn page_uri_match(candidate: &str, current: &str, expected_page: usize) -> PageUriMatch {
    let (Ok(candidate), Ok(current)) = (Url::parse(candidate), Url::parse(current)) else {
        return PageUriMatch::Different;
    };
    if candidate.scheme() != current.scheme()
        || candidate.host_str() != current.host_str()
        || candidate.port_or_known_default() != current.port_or_known_default()
        || candidate.path() != current.path()
    {
        return PageUriMatch::Different;
    }
    let query = |url: &Url| {
        url.query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect::<BTreeMap<_, _>>()
    };
    let mut expected = query(&current);
    expected.insert("page".to_owned(), expected_page.to_string());
    let mut candidate = query(&candidate);
    if candidate == expected {
        return PageUriMatch::Exact;
    }
    if !expected.contains_key("attest")
        && candidate.remove("attest").as_deref() == Some("true")
        && candidate == expected
    {
        PageUriMatch::Attested
    } else {
        PageUriMatch::Different
    }
}

fn response_metadata(group: &CaptureGroup) -> Option<http::ResponseMetadata> {
    group.response.as_ref().and_then(StoredResponse::metadata)
}

fn is_server_challenge(group: &CaptureGroup) -> bool {
    let Some(response) = &group.response else {
        return false;
    };
    let Some(metadata) = response.metadata() else {
        return false;
    };
    if metadata.status == 403
        && metadata
            .header("cf-mitigated")
            .is_some_and(|value| value.eq_ignore_ascii_case(b"challenge"))
    {
        return true;
    }
    if metadata.status != 454 {
        return false;
    }
    payload::entity_body(&response.body).is_ok_and(|entity| {
        contains_bytes(&entity, b"sc-challenge") && contains_bytes(&entity, b"/.sc-verify/")
    })
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn numeric_header(metadata: &http::ResponseMetadata, name: &str) -> Option<usize> {
    metadata
        .header(name)
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.trim().parse().ok())
}

const fn is_success(status: u16) -> bool {
    (status >= 200 && status < 300) || status == 304
}

fn endpoint_url(site: &Site, name: &str) -> String {
    format!("{}wp-json/wp/v2/{name}", site.root())
}

fn display_optional(value: Option<&str>) -> String {
    value.map_or_else(|| "no value".to_owned(), |value| format!("{value:?}"))
}

fn error(report: &mut Analysis, rule: &'static str, message: String) {
    report.findings.push(Custom::error(rule, message));
}

fn warning(report: &mut Analysis, rule: &'static str, message: String) {
    report.findings.push(Custom::warning(rule, message));
}

fn warning_once(report: &mut Analysis, rule: &'static str, message: &str) {
    if !report
        .findings
        .iter()
        .any(|finding| finding.severity() == Severity::Warning && finding.message() == message)
    {
        warning(report, rule, message.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        ATTEST_LINK_WARNING, Analysis, CaptureGroup, PageCapture, PageUriMatch, Probe, Severity,
        StoredResponse, StoredRevisit, StoredRevisitProfile, expected_initial,
        initial_capture_candidates, is_server_challenge, lint_series, page_query, page_uri_match,
    };
    use crate::archive::Site;
    use crate::endpoint::{Endpoint, ROOT_ENDPOINTS};
    use crate::evidence::StoredMetadata;

    const BEFORE: &str = "2026-08-20T00:00:00Z";

    fn endpoint() -> String {
        "https://example.com/wp-json/wp/v2/posts".to_owned()
    }

    fn page(number: usize) -> String {
        format!(
            "{}?before={BEFORE}&orderby=id&order=asc&page={number}&per_page=100",
            endpoint()
        )
    }

    fn capture(number: usize, via: &str, total: usize) -> CaptureGroup {
        let url = page(number);
        let links = [
            (number > 1).then(|| format!("<{}>; rel=\"prev\"", page(number - 1))),
            (number < total).then(|| format!("<{}>; rel=\"next\"", page(number + 1))),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        let link = (!links.is_empty()).then(|| format!("Link: {links}\r\n"));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
             X-WP-TotalPages: {total}\r\n{}Content-Length: 2\r\n\r\n[]",
            link.as_deref().unwrap_or("")
        );
        CaptureGroup {
            url: url.clone(),
            response: Some(StoredResponse {
                url: url.clone(),
                body: response.into_bytes(),
                truncation: None,
                revisit: None,
            }),
            metadata: vec![StoredMetadata {
                url: Some(url),
                via: Some(via.to_owned()),
                fields: true,
            }],
        }
    }

    fn root_capture(status: u16, body: &str) -> CaptureGroup {
        let url = "https://example.com/wp-json".to_owned();
        let response = format!(
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        CaptureGroup {
            url: url.clone(),
            response: Some(StoredResponse {
                url,
                body: response.into_bytes(),
                truncation: None,
                revisit: None,
            }),
            metadata: Vec::new(),
        }
    }

    fn revisit(number: usize, via: &str, total: usize, original: usize) -> CaptureGroup {
        let mut group = capture(number, via, total);
        let response = group.response.as_mut().expect("a stored response");
        let body_offset = response
            .body
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| position + 4)
            .expect("an HTTP response head");
        response.body.truncate(body_offset);
        response.truncation = Some("length".to_owned());
        response.revisit = Some(StoredRevisit {
            profile: StoredRevisitProfile::IdenticalPayloadDigest,
            original: Some(original),
            identified_json: true,
        });
        group
    }

    fn failed_capture(number: usize, via: &str, status: u16) -> CaptureGroup {
        let mut group = capture(number, via, 3);
        let response = group.response.as_mut().expect("a stored response");
        response.body = format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\n\r\n").into_bytes();
        group
    }

    fn truncated_capture(number: usize, via: &str) -> CaptureGroup {
        let mut group = capture(number, via, 3);
        group
            .response
            .as_mut()
            .expect("a stored response")
            .truncation = Some("time".to_owned());
        group
    }

    fn pages(groups: &[CaptureGroup]) -> Vec<PageCapture> {
        groups
            .iter()
            .enumerate()
            .map(|(group, capture)| {
                let (page, valid_query, _) = page_query(&capture.url);
                PageCapture {
                    group,
                    page,
                    valid_query,
                }
            })
            .collect()
    }

    fn probe() -> Probe {
        Probe {
            collection: Endpoint::Posts.into(),
            url: endpoint(),
            status: Some(200),
            items: Some(101),
        }
    }

    #[test]
    fn initial_capture_order_uses_the_supported_endpoint_order() {
        let site = Site::parse("example.com").expect("a site");
        let initial = expected_initial(&site, &[]);
        let endpoints = initial[ROOT_ENDPOINTS.len()..]
            .iter()
            .map(|(_, collection)| {
                collection
                    .as_ref()
                    .expect("an endpoint follows the roots")
                    .name()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            endpoints,
            [
                "pages",
                "posts",
                "media",
                "comments",
                "users",
                "categories",
                "tags",
                "navigation",
            ]
        );
    }

    #[test]
    fn simply_browser_challenges_do_not_count_as_initial_captures() {
        let challenge = root_capture(454, r"<script sc-challenge>fetch('/.sc-verify/')</script>");
        assert!(is_server_challenge(&challenge));

        let groups = [challenge, root_capture(200, "{}")];
        assert_eq!(
            initial_capture_candidates(&groups, "https://example.com/wp-json"),
            [1]
        );
    }

    #[test]
    fn ordinary_error_responses_still_count_as_initial_captures() {
        let groups = [
            root_capture(454, "ordinary server error"),
            root_capture(200, "{}"),
        ];
        assert_eq!(
            initial_capture_candidates(&groups, "https://example.com/wp-json"),
            [0, 1]
        );
    }

    #[test]
    fn paging_query_requires_the_archive_shape() {
        assert_eq!(
            page_query(&page(2)),
            (Some(2), true, Some(BEFORE.to_owned()))
        );
        assert!(!page_query(&format!("{}?page=2", endpoint())).1);
        assert!(!page_query(&page(0)).1);
        assert!(!page_query(&format!("{}&extra=yes", page(2))).1);
    }

    #[test]
    fn link_targets_are_compared_by_query_values() {
        let encoded = page(2).replace(BEFORE, "2026-08-20T00%3A00%3A00Z");
        assert_eq!(page_uri_match(&encoded, &page(1), 2), PageUriMatch::Exact);
        assert_eq!(
            page_uri_match(&format!("{encoded}&attest=true"), &page(1), 2),
            PageUriMatch::Attested
        );
        assert_eq!(
            page_uri_match(&format!("{encoded}&attest=false"), &page(1), 2),
            PageUriMatch::Different
        );
        assert_eq!(
            page_uri_match(&format!("{encoded}&other=true"), &page(1), 2),
            PageUriMatch::Different
        );
        assert_eq!(
            page_uri_match(&page(3), &page(1), 2),
            PageUriMatch::Different
        );
    }

    #[test]
    fn attested_pagination_links_produce_one_site_warning() {
        let probe = probe();
        let mut groups = vec![capture(1, &probe.url, 3)];
        let via = groups[0].url.clone();
        groups.push(capture(2, &via, 3));
        let via = groups[1].url.clone();
        groups.push(capture(3, &via, 3));
        for group in &mut groups {
            let response = group.response.as_mut().expect("a response");
            let text = String::from_utf8(response.body.clone()).expect("an HTTP message");
            response.body = text.replace(">; rel=", "&attest=true>; rel=").into_bytes();
        }
        let captures = pages(&groups);
        let mut report = Analysis::default();

        assert_eq!(
            lint_series(&groups, &probe, &captures, &mut HashSet::new(), &mut report),
            Some(3)
        );
        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
        assert_eq!(report.findings[0].severity(), Severity::Warning);
        assert_eq!(report.findings[0].message(), ATTEST_LINK_WARNING);
    }

    #[test]
    fn a_multi_page_series_requires_one_complete_pagination_pass() {
        let probe = probe();
        let mut groups = vec![capture(1, &probe.url, 2)];
        groups.push(capture(2, &groups[0].url, 2));
        let captures = pages(&groups);
        let mut report = Analysis::default();

        assert_eq!(
            lint_series(&groups, &probe, &captures, &mut HashSet::new(), &mut report,),
            Some(2)
        );
        assert!(report.findings.is_empty(), "{:?}", report.findings);

        let mut repeated_groups = vec![capture(1, &probe.url, 2)];
        repeated_groups.push(capture(2, &repeated_groups[0].url, 2));
        repeated_groups.push(revisit(1, &repeated_groups[1].url, 2, 0));
        repeated_groups.push(revisit(2, &repeated_groups[2].url, 2, 1));
        let repeated_captures = pages(&repeated_groups);
        let mut legacy = Analysis::default();
        lint_series(
            &repeated_groups,
            &probe,
            &repeated_captures,
            &mut HashSet::new(),
            &mut legacy,
        );
        assert!(legacy.findings.is_empty(), "{:?}", legacy.findings);
    }

    #[test]
    fn failed_page_attempts_do_not_consume_pagination_positions() {
        let probe = probe();
        let mut groups = vec![capture(1, &probe.url, 3)];
        let page_one = groups[0].url.clone();
        groups.push(failed_capture(2, &page_one, 503));
        groups.push(capture(2, &page_one, 3));
        let page_two = groups[2].url.clone();
        groups.push(truncated_capture(3, &page_two));
        groups.push(capture(3, &page_two, 3));
        let captures = pages(&groups);
        let mut report = Analysis::default();

        assert_eq!(
            lint_series(&groups, &probe, &captures, &mut HashSet::new(), &mut report,),
            Some(3)
        );
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    #[test]
    fn an_empty_collection_still_has_page_one() {
        let mut probe = probe();
        probe.items = Some(0);
        let groups = vec![capture(1, &probe.url, 0)];
        let captures = pages(&groups);
        let mut report = Analysis::default();

        assert_eq!(
            lint_series(&groups, &probe, &captures, &mut HashSet::new(), &mut report,),
            Some(0)
        );
        assert!(report.findings.is_empty(), "{:?}", report.findings);

        let mut groups = groups;
        groups.push(capture(1, &groups[0].url, 0));
        let captures = pages(&groups);
        let mut repeated = Analysis::default();
        lint_series(
            &groups,
            &probe,
            &captures,
            &mut HashSet::new(),
            &mut repeated,
        );
        assert!(repeated.findings.iter().any(|finding| {
            finding.message() == "posts pagination series has an unnecessary second pass"
        }));
    }
}
