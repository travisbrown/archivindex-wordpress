use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use archivindex_archiver::capture::CaptureSummary;
use archivindex_archiver::session::{Capture, Driver, Request};
use archivindex_cli_support::{CommandOutcome, config};
use archivindex_test_support::http::{RequestExt, Server, dead_port, response, serve_with};
use archivindex_test_support::warc::render;
use archivindex_warc::io::read::WarcReader;
use archivindex_warc::io::write::WarcWriter;
use archivindex_warc::parse::raw;
use archivindex_warc::record::extension::NoExtension;
use archivindex_warc::record::{FieldsBlock, Record};
use archivindex_warc_ops::lint::Severity;
use archivindex_wordpress_scraper::CommentDriver;
use archivindex_wordpress_scraper::archive::{
    ArchiveDriver, Checkpoint, DEFAULT_PER_PAGE, Resumption, Site,
};
use archivindex_wordpress_scraper::endpoint::{Collection, Endpoint, Registry};
use archivindex_wordpress_scraper::lint::lint_archive;
use archivindex_wordpress_scraper::read::{CommentCompleteness, check_comment_collections};
use archivindex_wordpress_scraper::resume::{inspect_archive, inspect_archive_with_config};
use chrono::{DateTime, Utc};
use clap::{CommandFactory, Parser};
use flate2::Compression;
use flate2::write::GzEncoder;

use super::{
    ArchiveProgress, ArchiveRunOptions, ArchiveRunState, CheckCommentsOptions, CombineOptions,
    Command, CommentRun, CommentRunOptions, Config, Error, Opts, PerPageOptions,
    ResumeArchiveOptions, ResumeInfoOptions, capture_comment_run, check_wp_comments,
    combine_archives, comment_update_inputs, load_config_for_output, page_total_change_warning,
    parse_per_page, resume_archive, resume_command, resume_info, run_archive,
    session_name_from_warc,
};

const BEFORE: &str = "2026-08-20T00:00:00Z";

fn before() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(BEFORE)
        .map(|date| date.with_timezone(&Utc))
        .expect("a test timestamp")
}

fn resumption(
    endpoint: impl Into<Collection>,
    last_page: usize,
    total_pages: Option<usize>,
) -> Resumption {
    Resumption {
        endpoint: endpoint.into(),
        last_page,
        total_pages,
    }
}

fn custom(name: &str, registry: Registry) -> Collection {
    Collection::Custom {
        name: name.to_owned(),
        registry,
    }
}

/// A registry response with one `wp/v2` entry whose collection is at `rest_base`.
fn registry(rest_base: &str) -> String {
    format!(
        r#"{{"{rest_base}": {{"name": "", "description": "", "hierarchical": false,
                "slug": "{rest_base}", "rest_base": "{rest_base}", "rest_namespace": "wp/v2",
                "_links": {{"wp:items": [{{"href": "https://example.com/x"}}]}}}}}}"#
    )
}

/// Serve `requests` of a site exposing two pages of `pages`, one of `comments`, and one of the
/// custom `videos` type its type registry advertises.
///
/// The taxonomy registry advertises `series`, which is not exposed; every other probe is
/// answered with 404, and each note is the request's path.
fn serve_site(requests: usize) -> std::io::Result<Server<String>> {
    serve_with(requests, |request| {
        let target = request.path();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let json = [("content-type", "application/json")];
        let reply = match path.strip_prefix("/wp-json/wp/v2/") {
            Some("types") => response(200, &json, registry("videos")),
            Some("taxonomies") => response(200, &json, registry("series")),
            Some("pages") if query.is_empty() => response(
                200,
                &[
                    ("content-type", "application/json"),
                    ("x-wp-total", "101"),
                    ("x-wp-totalpages", "2"),
                ],
                "[]",
            ),
            Some("comments" | "videos") if query.is_empty() => response(
                200,
                &[
                    ("content-type", "application/json"),
                    ("x-wp-total", "3"),
                    ("x-wp-totalpages", "1"),
                ],
                "[]",
            ),
            Some("pages") => response(
                200,
                &[
                    ("content-type", "application/json"),
                    ("x-wp-totalpages", "2"),
                ],
                "[]",
            ),
            Some("comments" | "videos") => response(
                200,
                &[
                    ("content-type", "application/json"),
                    ("x-wp-totalpages", "1"),
                ],
                "[]",
            ),
            Some(_) => response(404, &json, "{}"),
            None => response(200, &json, "{}"),
        };
        (reply, target.to_owned())
    })
}

fn archive_options(port: u16, output: &Path, session_name: &str) -> ArchiveRunOptions {
    ArchiveRunOptions {
        config: None,
        base: Site::parse(&format!("http://127.0.0.1:{port}")).expect("a site"),
        output: output.to_owned(),
        session_name: Some(session_name.to_owned()),
        revisit_index: None,
        limit: None,
        per_page: PerPageOptions::default(),
        cookie: None,
    }
}

fn write_update_warc(
    path: &Path,
    base_url: &str,
    comment_datetime: &str,
    gzip: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = format!(r#"[{{"id":1,"date_gmt":"{comment_datetime}"}}]"#);
    write_update_warc_batches(
        path,
        &[(base_url, "2026-08-20T00:00:00Z", body.as_str())],
        gzip,
    )
}

fn write_update_warc_batches(
    path: &Path,
    batches: &[(&str, &str, &str)],
    gzip: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    let mut writer = WarcWriter::new(&mut bytes);
    for (base_url, before, body) in batches {
        let url = format!("{base_url}wp-json/wp/v2/comments?before={before}&page=1");
        let message = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\n\r\n{body}",
            body.len()
        );
        let record: Record = Record::response(&url, Utc::now())?.body(message.into_bytes())?;
        writer.write(&record.into_raw()?)?;
    }
    writer.flush()?;
    if gzip {
        let mut encoder = GzEncoder::new(std::fs::File::create(path)?, Compression::default());
        encoder.write_all(&bytes)?;
        encoder.finish()?;
    } else {
        std::fs::write(path, bytes)?;
    }

    Ok(())
}

/// A metadata record's target URL and `via`.
type MetadataVia = (String, Option<String>);

fn assert_archive_lint(warc: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let lint = lint_archive(warc)?;
    let records = WarcReader::from_path(warc)?
        .iter_records::<NoExtension>()
        .records()
        .collect::<Result<Vec<_>, _>>()?
        .len();
    assert_eq!(lint.core_lints_passed, records);
    assert_eq!(
        (lint.roots, lint.known_probes, lint.custom_probes),
        (8, 8, 2)
    );
    assert_eq!(lint.error_count(), 0, "{:?}", lint.findings);
    assert_eq!(lint.warning_count(), 2);
    assert!(
        lint.findings
            .iter()
            .all(|finding| finding.violation.severity() == Severity::Warning)
    );
    assert_eq!(
        lint.pagination
            .iter()
            .map(|summary| (
                summary.endpoint.rsplit('/').next(),
                summary.pages,
                summary.items
            ))
            .collect::<Vec<_>>(),
        [
            (Some("pages"), Some(2), Some(101)),
            (Some("comments"), Some(1), Some(3)),
            (Some("videos"), Some(1), Some(3)),
        ]
    );

    // Compressing gives each record a gzip member of its own and names the output in the
    // warcinfo record, so the compressed archive is held to the same rules and reports the same
    // findings.
    let gzip = warc.with_extension("warc.gz");
    archivindex_warc_ops::compress::compress_path(warc, 6, &gzip)?;
    assert_eq!(lint_archive(gzip)?, lint);
    Ok(())
}

/// Serve `requests` of a two-page comments collection on a local port.
fn serve_comment_pages(requests: usize) -> std::io::Result<Server<String>> {
    serve_with(requests, |request| {
        let target = request.path();
        let page = url::Url::parse(&format!("http://localhost{target}"))
            .expect("a request URL")
            .query_pairs()
            .find_map(|(name, value)| (name == "page").then(|| value.into_owned()))
            .expect("a page parameter");
        let body = format!(
            r#"[{{"id":{page},"post":1,"parent":0,"author":0,"author_name":"Example","author_url":"","date":"2026-08-20T00:00:0{page}","date_gmt":"2026-08-20T00:00:0{page}","content":{{"rendered":"Example comment"}},"link":"http://localhost/post/#comment-{page}","status":"approved","type":"comment","meta":[],"_links":{{}}}}]"#
        );
        let headers = [
            ("content-type", "application/json"),
            ("x-wp-total", "2"),
            ("x-wp-totalpages", "2"),
        ];
        (response(200, &headers, &body), target.to_owned())
    })
}

#[test]
fn archive_command_reads_workflow_and_config_options() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "archive",
        "--config",
        "capture.toml",
        "--base",
        "example.com/blog/",
        "-o",
        "archives",
        "--session-name",
        "blog-2026",
        "--revisit-index",
        "state.sqlite3",
        "--limit",
        "12",
        "--per-page",
        "20",
        "--per-page",
        "media:2",
        "--per-page",
        "plugin-items:5",
        "--cookie",
        "cf_clearance=test-clearance; __cf_bm=test-bot-cookie",
    ])
    .expect("valid options");

    let Command::Archive(options) = options.command else {
        panic!("expected the archiving command");
    };

    assert_eq!(options.base.base(), "example.com/blog");
    assert_eq!(options.output, PathBuf::from("archives"));
    assert_eq!(options.session_name.as_deref(), Some("blog-2026"));
    assert_eq!(options.config, Some(PathBuf::from("capture.toml")));
    assert_eq!(options.revisit_index, Some(PathBuf::from("state.sqlite3")));
    assert_eq!(options.limit, Some(12));
    assert_eq!(options.per_page.default_value(), 20);
    assert_eq!(
        options.per_page.endpoint_values(),
        BTreeMap::from([("media", 2), ("plugin-items", 5)])
    );
    assert_eq!(
        options.cookie.as_deref(),
        Some("cf_clearance=test-clearance; __cf_bm=test-bot-cookie")
    );

    let defaults = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "archive",
        "--base",
        "example.com",
        "-o",
        "archives",
    ])
    .expect("valid options");
    let Command::Archive(defaults) = defaults.command else {
        panic!("expected the archiving command");
    };
    assert_eq!(defaults.session_name, None);
    assert_eq!(defaults.per_page.default_value(), DEFAULT_PER_PAGE);
    assert!(defaults.per_page.endpoint_values().is_empty());
    assert!(
        defaults
            .base
            .session_name(before())
            .starts_with("example.com-")
    );
}

#[test]
fn lint_command_accepts_a_warc_path() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "lint",
        "-i",
        "archives/site.warc.gz",
    ])
    .expect("valid options");
    let Command::Lint(options) = options.command else {
        panic!("expected the lint command");
    };

    assert_eq!(options.input, PathBuf::from("archives/site.warc.gz"));
}

#[test]
fn combine_command_reads_its_domain_and_paths() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "combine",
        "-i",
        "archives",
        "--domain",
        "example.com",
        "-o",
        "example.com.warc.gz",
    ])
    .expect("valid options");
    let Command::Combine(options) = options.command else {
        panic!("expected the combine command");
    };

    assert_eq!(options.input, PathBuf::from("archives"));
    assert_eq!(options.domain, "example.com");
    assert_eq!(options.output, PathBuf::from("example.com.warc.gz"));
}

#[test]
fn combine_joins_plain_and_gzip_resume_segments_in_filename_order()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("archives");
    std::fs::create_dir(&input)?;
    write_update_warc(
        &input.join("example.com-200.warc.gz"),
        "https://example.com/",
        "2026-08-20T00:00:02",
        true,
    )?;
    write_update_warc(
        &input.join("example.com-100.warc"),
        "https://example.com/",
        "2026-08-20T00:00:01",
        false,
    )?;
    write_update_warc(
        &input.join("other.example-150.warc"),
        "https://other.example/",
        "2026-08-20T00:00:03",
        false,
    )?;
    let output = directory.path().join("example.com.warc.gz");

    let summary = combine_archives(&CombineOptions {
        input,
        domain: "example.com".to_owned(),
        output: output.clone(),
    })?;

    assert_eq!((summary.files, summary.records), (2, 2));
    assert_eq!(&std::fs::read(&output)?[..2], &[0x1f, 0x8b]);
    let located = WarcReader::from_path_gzip(&output)?
        .iter_raw_records()
        .collect::<Vec<_>>();
    assert!(located.iter().all(|record| record.frame().is_some()));
    let bodies = located
        .into_iter()
        .map(|record| record.value.map(|record| record.body))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(bodies.len(), 2);
    assert!(
        bodies[0]
            .windows(19)
            .any(|part| part == b"2026-08-20T00:00:01")
    );
    assert!(
        bodies[1]
            .windows(19)
            .any(|part| part == b"2026-08-20T00:00:02")
    );

    Ok(())
}

#[test]
fn combine_writes_plain_warc_and_consolidates_warcinfo() -> Result<(), Box<dyn std::error::Error>> {
    const FIRST_ID: &str = "<urn:uuid:aaaaaaaa-0000-4000-8000-000000000000>";
    const LATER_ID: &str = "<urn:uuid:bbbbbbbb-0000-4000-8000-000000000000>";
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("archives");
    std::fs::create_dir(&input)?;

    let mut first = render(
        &[
            ("WARC-Type", "warcinfo"),
            ("WARC-Record-ID", FIRST_ID),
            ("WARC-Date", "2026-08-20T00:00:00Z"),
            ("WARC-Filename", "example.com-100.warc"),
            ("Content-Type", "application/warc-fields"),
        ],
        "software: test/1.0\r\n",
    );
    first.extend_from_slice(&render(
        &[
            ("WARC-Type", "metadata"),
            (
                "WARC-Record-ID",
                "<urn:uuid:cccccccc-0000-4000-8000-000000000000>",
            ),
            ("WARC-Date", "2026-08-20T00:00:01Z"),
            ("WARC-Concurrent-To", LATER_ID),
        ],
        "via: https://example.com/\r\n",
    ));
    std::fs::write(input.join("example.com-100.warc"), first)?;

    let mut later = render(
        &[
            ("WARC-Type", "warcinfo"),
            ("WARC-Record-ID", LATER_ID),
            ("WARC-Date", "2026-08-20T00:00:02Z"),
            ("WARC-Filename", "example.com-200.warc"),
            ("Content-Type", "application/warc-fields"),
        ],
        "software: test/2.0\r\n",
    );
    later.extend_from_slice(&render(
        &[
            ("WARC-Type", "metadata"),
            (
                "WARC-Record-ID",
                "<urn:uuid:dddddddd-0000-4000-8000-000000000000>",
            ),
            ("WARC-Date", "2026-08-20T00:00:03Z"),
            ("WARC-Warcinfo-ID", LATER_ID),
            ("WARC-Refers-To", LATER_ID),
            ("WARC-Segment-Origin-ID", LATER_ID),
        ],
        "via: https://example.com/\r\n",
    ));
    std::fs::write(input.join("example.com-200.warc"), later)?;

    let output = directory.path().join("combined.warc");
    let summary = combine_archives(&CombineOptions {
        input,
        domain: "example.com".to_owned(),
        output: output.clone(),
    })?;

    assert_eq!((summary.files, summary.records), (2, 3));
    assert_eq!(&std::fs::read(&output)?[..5], b"WARC/");
    let records = WarcReader::from_path(&output)?
        .iter_raw_records()
        .records()
        .collect::<Result<Vec<raw::Record>, _>>()?;
    assert_eq!(records.len(), 3);
    assert_eq!(trimmed_header(&records[0], "WARC-Type"), Some("warcinfo"));
    assert_eq!(
        trimmed_header(&records[0], "WARC-Filename"),
        Some("combined.warc")
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| trimmed_header(record, "WARC-Type") == Some("warcinfo"))
            .count(),
        1
    );
    for (record, fields) in [
        (&records[1], &["WARC-Concurrent-To"][..]),
        (
            &records[2],
            &[
                "WARC-Warcinfo-ID",
                "WARC-Refers-To",
                "WARC-Segment-Origin-ID",
            ][..],
        ),
    ] {
        for field in fields {
            assert_eq!(trimmed_header(record, field), Some(FIRST_ID));
        }
    }

    Ok(())
}

fn trimmed_header<'a>(record: &'a raw::Record, name: &str) -> Option<&'a str> {
    std::str::from_utf8(record.header.get(name)?.trim_ascii()).ok()
}

#[test]
fn resume_command_requires_only_the_output_and_session_name() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "resume-archive",
        "--config",
        "capture.toml",
        "--output",
        "archives",
        "--session-name",
        "example.com",
        "--revisit-index",
        "state.sqlite3",
        "--limit",
        "12",
        "--per-page",
        "10",
        "--per-page",
        "comments:3",
        "--cookie",
        "secret=yes",
    ])
    .expect("valid options");

    let Command::ResumeArchive(options) = options.command else {
        panic!("expected the resuming command");
    };

    assert_eq!(options.output, PathBuf::from("archives"));
    assert_eq!(options.session_name, "example.com");
    assert_eq!(options.config, Some(PathBuf::from("capture.toml")));
    assert_eq!(options.revisit_index, Some(PathBuf::from("state.sqlite3")));
    assert_eq!(options.limit, Some(12));
    assert_eq!(options.per_page.default_value(), 10);
    assert_eq!(
        options.per_page.endpoint_values(),
        BTreeMap::from([("comments", 3)])
    );
    assert_eq!(options.cookie.as_deref(), Some("secret=yes"));

    for arguments in [
        vec![
            "archivindex-wordpress-scraper",
            "resume-archive",
            "--session-name",
            "example.com",
        ],
        vec![
            "archivindex-wordpress-scraper",
            "resume-archive",
            "--output",
            "archives",
        ],
    ] {
        assert!(Opts::try_parse_from(arguments).is_err());
    }
}

#[test]
fn resume_info_command_takes_a_warc_path() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "resume-info",
        "--input",
        "archives/site.warc.gz",
    ])
    .expect("valid options");

    let Command::ResumeInfo(options) = options.command else {
        panic!("expected the resume information command");
    };
    assert_eq!(options.input, PathBuf::from("archives/site.warc.gz"));
}

#[test]
fn resume_info_derives_default_and_explicit_session_names() {
    assert_eq!(
        session_name_from_warc(Path::new("archives/example.com-1788032113.warc")).as_deref(),
        Some("example.com")
    );
    assert_eq!(
        session_name_from_warc(Path::new(
            "archives/example.com-1788032113~1788032999.warc.gz"
        ),)
        .as_deref(),
        Some("example.com")
    );
    assert_eq!(
        session_name_from_warc(Path::new("archives/editorial~nightly.warc")).as_deref(),
        Some("editorial~nightly")
    );
    assert_eq!(
        session_name_from_warc(Path::new("archives/editorial~nightly~1788032999.warc",)).as_deref(),
        Some("editorial~nightly")
    );
    assert_eq!(
        session_name_from_warc(Path::new("archives/editorial-nightly-1788032999.warc")).as_deref(),
        Some("editorial-nightly")
    );
    assert_eq!(
        session_name_from_warc(Path::new("archives/campaign-2026.warc")).as_deref(),
        Some("campaign-2026")
    );
}

#[test]
fn every_archive_segment_name_has_a_numeric_timestamp() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let name = super::SessionInventory::read(directory.path())
        .unwrap()
        .next_name("editorial-nightly")
        .unwrap();
    let timestamp = name
        .strip_prefix("editorial-nightly-")
        .expect("the session prefix");
    assert!(!timestamp.is_empty());
    assert!(timestamp.bytes().all(|byte| byte.is_ascii_digit()));
}

#[test]
fn a_session_is_printed_as_a_minimal_resume_command() {
    assert_eq!(
        resume_command(
            Path::new("archives"),
            "example.com",
            None,
            None,
            None,
            &PerPageOptions::default(),
        ),
        "archivindex-wordpress-scraper resume-archive --output archives --session-name example.com"
    );

    let configured = PerPageOptions {
        values: ["20", "media:2", "plugin-items:5"]
            .map(|value| parse_per_page(value).expect("a page-size setting"))
            .to_vec(),
    };
    assert!(
        resume_command(
            Path::new("archives"),
            "example.com",
            None,
            None,
            None,
            &configured,
        )
        .ends_with(" --per-page 20 --per-page media:2 --per-page plugin-items:5")
    );

    assert_eq!(
        resume_command(
            Path::new("archive output/it's here"),
            "site's run",
            Some(Path::new("capture files/site's.toml")),
            Some(Path::new("state files/site.sqlite3")),
            Some(before()),
            &PerPageOptions::default(),
        ),
        "archivindex-wordpress-scraper resume-archive --output 'archive output/it'\"'\"'s here' \
             --session-name 'site'\"'\"'s run' --config 'capture files/site'\"'\"'s.toml' \
             --revisit-index 'state files/site.sqlite3' --before 2026-08-20T00:00:00Z"
    );
}

fn recorded_capture(
    url: &str,
    truncated: Option<archivindex_warc::record::header::truncated_type::TruncatedType>,
) -> CaptureSummary {
    CaptureSummary {
        url: url.to_owned(),
        origin: archivindex_archiver::capture::Origin::Seed,
        date: before(),
        status: 200,
        size: 2,
        redirects: 0,
        truncated,
    }
}

#[test]
fn archive_checkpoint_stays_before_the_first_partial_or_rejected_capture() {
    use archivindex_warc::record::header::truncated_type::TruncatedType;

    for (accepted, truncated) in [
        (true, Some(TruncatedType::Disconnect)),
        (true, Some(TruncatedType::Length)),
        (true, None),
        (false, None),
    ] {
        let initial = resumption(Endpoint::Comments, 1, Some(4));
        let mut state = ArchiveRunState::new(ArchiveDriver::resume(
            Site::parse("example.com").expect("a site"),
            before(),
            initial.clone(),
            Vec::new(),
        ));
        for page in 2..=3 {
            let request = state.next().expect("next page");
            let response = b"HTTP/1.1 200 OK\r\nX-WP-TotalPages: 4\r\n\r\n";
            let capture =
                Capture::new(&request.url, &request.url, b"[]", response).expect("a response");
            assert_eq!(state.inspect(&capture).error, None);
            let recorded = recorded_capture(
                &request.url,
                if page == 2 { truncated.clone() } else { None },
            );
            state.recorded((accepted || page == 3).then_some(&recorded));
        }
        let expected = if !accepted || truncated == Some(TruncatedType::Disconnect) {
            initial
        } else {
            resumption(Endpoint::Comments, 3, Some(4))
        };
        assert_eq!(state.recorded, Checkpoint::Resume(expected));
    }
}

#[test]
fn archive_progress_advances_only_after_recording() {
    let driver = ArchiveDriver::resume(
        Site::parse("example.com").expect("a site"),
        before(),
        resumption(Endpoint::Comments, 1, Some(2)),
        Vec::new(),
    );
    let mut state = ArchiveRunState::new(driver);
    let url = format!(
        "https://example.com/wp-json/wp/v2/comments?before={BEFORE}&orderby=id&order=asc\
             &page=2&per_page=100"
    );
    let response = b"HTTP/1.1 200 OK\r\nX-WP-TotalPages: 2\r\n\r\n";
    let capture = Capture::new(&url, &url, b"[]", response).expect("a complete response");

    let inspection = state.inspect(&capture);

    assert_eq!(
        state.recorded,
        Checkpoint::Resume(resumption(Endpoint::Comments, 1, Some(2)))
    );
    assert_eq!(inspection.error, None);
    state.recorded(Some(&recorded_capture(&url, None)));
    assert_eq!(
        state.next(),
        Some(Request::seed("https://example.com/wp-json/wp/v2/users"))
    );

    assert_eq!(
        state.recorded,
        Checkpoint::Resume(resumption(Endpoint::Users, 0, None))
    );
}

#[test]
fn resumed_archive_progress_starts_at_its_checkpoint() {
    let mut driver = ArchiveDriver::resume(
        Site::parse("example.com").expect("a site"),
        before(),
        resumption(Endpoint::Comments, 7, Some(8)),
        Vec::new(),
    );

    let mut progress = ArchiveProgress::new(&driver);
    let comments = progress
        .pagination
        .get("comments")
        .expect("the resumed collection has a progress bar");
    assert_eq!(comments.position(), 7);
    assert_eq!(comments.length(), Some(8));

    let url = format!(
        "https://example.com/wp-json/wp/v2/comments?before={BEFORE}&orderby=id&order=asc\
             &page=8&per_page=100"
    );
    let response = b"HTTP/1.1 200 OK\r\nX-WP-TotalPages: 8\r\n\r\n";
    let capture = Capture::new(&url, &url, b"[]", response).expect("a complete response");
    let _ = driver.inspect(&capture);
    progress.update(&driver);

    assert_eq!(
        progress
            .pagination
            .get("comments")
            .expect("the resumed progress bar remains present")
            .position(),
        8
    );
    progress.finish();
}

#[test]
fn archive_command_does_not_duplicate_configuration_fields() {
    let command = Opts::command();
    let archive = command
        .find_subcommand("archive")
        .expect("the archive command");
    let argument_ids = archive
        .get_arguments()
        .map(|argument| argument.get_id().as_str())
        .collect::<Vec<_>>();

    for removed in [
        "gzip",
        "user_agent",
        "timeout",
        "max_redirects",
        "max_response_length",
        "operator",
        "operator_email",
        "retry_attempts",
        "retry_initial_backoff",
        "retry_max_backoff",
        "request_delay",
    ] {
        assert!(!argument_ids.contains(&removed), "unexpected --{removed}");
    }
}

#[test]
fn configuration_file_supplies_archiver_and_session_settings() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("capture.toml");
    std::fs::write(
        &path,
        "gzip-warc = true\n\
             [operator]\nname = \"A. Archivist\"\nemail = \"archivist@example.com\"\n\
             [session]\nrequest-delay = \"750ms\"\n",
    )
    .expect("write the configuration");

    let config = config::load::<Config>(Some(&path)).expect("read the configuration");

    assert!(config.gzip_warc);
    let operator = config.operator.expect("a configured operator");
    assert_eq!(operator.name, "A. Archivist");
    assert_eq!(operator.email.as_deref(), Some("archivist@example.com"));
    assert_eq!(config.session.request_delay, Duration::from_millis(750));
}

#[test]
fn output_filename_controls_capture_compression() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("capture.toml");
    std::fs::write(&path, "gzip-warc = true\n").expect("write the configuration");

    assert!(
        load_config_for_output(Some(&path), Path::new("comments.warc.gz"))
            .expect("load gzip output settings")
            .gzip_warc
    );
    assert!(
        !load_config_for_output(Some(&path), Path::new("comments.warc"))
            .expect("load plain output settings")
            .gzip_warc
    );
    assert!(
        load_config_for_output(None, Path::new("comments.warc.GZ"))
            .expect("load case-insensitive gzip output settings")
            .gzip_warc
    );
}

#[test]
fn read_command_takes_a_warc_path() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "read-comments",
        "-i",
        "comments.warc.gz",
    ])
    .expect("valid options");

    let Command::Read(options) = options.command else {
        panic!("expected the reading command");
    };

    assert_eq!(options.input, PathBuf::from("comments.warc.gz"));
}

#[test]
fn check_command_takes_a_warc_path() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "check-comments",
        "--input",
        "comments.warc.gz",
    ])
    .expect("valid options");

    let Command::Check(options) = options.command else {
        panic!("expected the checking command");
    };

    assert_eq!(options.input, PathBuf::from("comments.warc.gz"));
}

#[test]
fn complete_command_takes_input_and_output_warc_paths() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "complete-comments",
        "-i",
        "comments.warc.gz",
        "-o",
        "completion.warc.gz",
        "-c",
        "capture.toml",
    ])
    .expect("valid options");

    let Command::Complete(options) = options.command else {
        panic!("expected the completion command");
    };

    assert_eq!(options.input, PathBuf::from("comments.warc.gz"));
    assert_eq!(options.output, PathBuf::from("completion.warc.gz"));
    assert_eq!(options.config, Some(PathBuf::from("capture.toml")));
}

#[test]
fn path_arguments_are_never_positional() {
    for arguments in [
        vec!["archivindex-wordpress-scraper", "lint", "comments.warc.gz"],
        vec![
            "archivindex-wordpress-scraper",
            "resume-info",
            "comments.warc.gz",
        ],
        vec![
            "archivindex-wordpress-scraper",
            "read-comments",
            "comments.warc.gz",
        ],
        vec![
            "archivindex-wordpress-scraper",
            "check-comments",
            "comments.warc.gz",
        ],
        vec![
            "archivindex-wordpress-scraper",
            "complete-comments",
            "comments.warc.gz",
            "completion.warc.gz",
        ],
        vec![
            "archivindex-wordpress-scraper",
            "update-comments",
            "comments.warc.gz",
            "--output",
            "update.warc.gz",
            "--session-name",
            "comments-update",
        ],
    ] {
        assert!(
            Opts::try_parse_from(arguments).is_err(),
            "a positional path was accepted"
        );
    }
}

#[test]
fn update_command_uses_a_one_day_default_overlap() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "update-comments",
        "--input",
        "historical.warc.gz",
        "-o",
        "update.warc.gz",
        "--session-name",
        "comments-update-2026-08-20",
    ])
    .expect("valid options");

    let Command::Update(options) = options.command else {
        panic!("expected the update command");
    };

    assert_eq!(options.input, PathBuf::from("historical.warc.gz"));
    assert_eq!(options.output, PathBuf::from("update.warc.gz"));
    assert_eq!(options.session_name, "comments-update-2026-08-20");
    assert_eq!(options.overlap, Duration::from_hours(24));
}

#[test]
fn update_command_parses_a_configured_overlap() {
    let options = Opts::try_parse_from([
        "archivindex-wordpress-scraper",
        "update-comments",
        "-i",
        "historical.warc",
        "--output",
        "update.warc",
        "--session-name",
        "comments-update",
        "--overlap",
        "36hours",
    ])
    .expect("valid options");

    let Command::Update(options) = options.command else {
        panic!("expected the update command");
    };
    assert_eq!(options.overlap, Duration::from_hours(36));
}

#[test]
fn update_directory_reads_only_direct_warcs_in_domain_order()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let nested = directory.path().join("nested");
    std::fs::create_dir(&nested)?;
    write_update_warc(
        &directory.path().join("zeta.warc.gz"),
        "https://zeta.example/",
        "2026-08-18T00:00:00",
        true,
    )?;
    write_update_warc(
        &directory.path().join("alpha.warc"),
        "https://alpha.example/blog/",
        "2026-08-19T00:00:00",
        false,
    )?;
    write_update_warc(
        &directory.path().join("ignored.data"),
        "https://ignored.example/",
        "2026-08-19T00:00:00",
        false,
    )?;
    write_update_warc(
        &nested.join("nested.warc"),
        "https://nested.example/",
        "2026-08-19T00:00:00",
        false,
    )?;

    let updates = comment_update_inputs(directory.path())?;

    assert_eq!(
        updates
            .iter()
            .map(|update| update.anchor.base_url.as_str())
            .collect::<Vec<_>>(),
        ["https://alpha.example/blog/", "https://zeta.example/"]
    );
    assert_eq!(
        updates
            .iter()
            .filter_map(|update| update.path.file_name().and_then(|name| name.to_str()))
            .collect::<Vec<_>>(),
        ["alpha.warc", "zeta.warc.gz"]
    );

    Ok(())
}

#[test]
fn update_directory_merges_prior_multi_site_updates_by_site()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    write_update_warc(
        &directory.path().join("alpha-history.warc"),
        "https://alpha.example/",
        "2026-08-18T00:00:00",
        false,
    )?;
    write_update_warc(
        &directory.path().join("zeta-history.warc"),
        "https://zeta.example/",
        "2026-08-17T00:00:00",
        false,
    )?;
    write_update_warc_batches(
        &directory.path().join("first-update.warc"),
        &[
            (
                "https://beta.example/",
                "2026-08-22T00:00:00Z",
                r#"[{"id":1,"date_gmt":"2026-08-21T00:00:00"}]"#,
            ),
            (
                "https://alpha.example/",
                "2026-08-22T00:00:00Z",
                r#"[{"id":2,"date_gmt":"2026-08-20T00:00:00"}]"#,
            ),
        ],
        false,
    )?;
    // A later empty run must not displace the latest actual comment for this site.
    write_update_warc_batches(
        &directory.path().join("empty-update.warc"),
        &[("https://alpha.example/", "2026-08-25T00:00:00Z", "[]")],
        false,
    )?;

    let updates = comment_update_inputs(directory.path())?;

    assert_eq!(
        updates
            .iter()
            .map(|update| update.anchor.base_url.as_str())
            .collect::<Vec<_>>(),
        [
            "https://alpha.example/",
            "https://beta.example/",
            "https://zeta.example/"
        ]
    );
    assert_eq!(
        updates[0].anchor.datetime.to_rfc3339(),
        "2026-08-20T00:00:00+00:00"
    );
    assert!(updates[0].anchor.from_comment);
    assert_eq!(
        updates[0].path.file_name().and_then(|name| name.to_str()),
        Some("first-update.warc")
    );

    Ok(())
}

#[test]
fn update_directory_requires_a_direct_warc() {
    let directory = tempfile::tempdir().expect("a temporary directory");

    assert!(matches!(
        comment_update_inputs(directory.path()),
        Err(Error::NoUpdateWarcs(path)) if path == directory.path()
    ));
}

#[test]
fn multi_domain_update_starts_each_via_chain_at_its_own_first_page()
-> Result<(), Box<dyn std::error::Error>> {
    let first_server = serve_comment_pages(2)?;
    let second_server = serve_comment_pages(2)?;
    let mut base_urls = [
        format!("http://127.0.0.1:{}/", first_server.port()),
        format!("http://127.0.0.1:{}/", second_server.port()),
    ];
    base_urls.sort();
    let before = chrono::DateTime::parse_from_rfc3339("2026-08-21T00:00:00Z")?.with_timezone(&Utc);
    let after = chrono::DateTime::parse_from_rfc3339("2026-08-19T00:00:00Z")?.with_timezone(&Utc);
    let runs = base_urls
        .iter()
        .map(|base_url| {
            Ok(CommentRun {
                site_url: base_url.clone(),
                driver: CommentDriver::for_window(base_url, after, before)?,
            })
        })
        .collect::<Result<Vec<_>, url::ParseError>>()?;
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("updates.warc");

    let outcome = capture_comment_run(
        runs,
        CommentRunOptions {
            config: None,
            cookie: None,
            output: &output,
            session_name: "multi-domain-update",
            revisit_index: None,
            limit: None,
            second_sweep: false,
        },
        true,
    )?;
    assert_eq!(outcome, archivindex_cli_support::CommandOutcome::Success);
    let _ = first_server.finish();
    let _ = second_server.finish();

    let metadata = metadata_vias(&output)?;
    for base_url in base_urls {
        let driver = CommentDriver::for_window(&base_url, after, before)?;
        let first = driver.first_comment_url();
        let second = first.replace("&page=1&", "&page=2&");
        assert!(metadata.contains(&(first.clone(), None)));
        assert!(metadata.contains(&(second, Some(first))));
    }
    let collections = check_comment_collections(&output)?;
    assert_eq!(collections.len(), 2);
    assert!(collections.iter().all(|collection| {
        collection.coverage.total_pages == Some(2)
            && collection.coverage.captured_pages == [1, 2]
            && collection.coverage.is_complete()
    }));
    assert_eq!(
        check_wp_comments(&CheckCommentsOptions { input: output }, true,)?,
        archivindex_cli_support::CommandOutcome::Success
    );

    Ok(())
}

#[test]
fn archive_site_timestamps_an_explicit_session_name() -> Result<(), Box<dyn std::error::Error>> {
    let server = serve_site(22)?;
    let port = server.port();
    let directory = tempfile::tempdir()?;
    let options = archive_options(port, directory.path(), "editorial-nightly");

    assert_eq!(
        super::archive_site(&options, true)?,
        CommandOutcome::Success
    );
    assert_eq!(server.finish().len(), 22);
    let files = super::SessionInventory::read(directory.path())?.warcs("editorial-nightly")?;
    assert_eq!(files.len(), 1);
    let name = files[0]
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a UTF-8 filename");
    let timestamp = name
        .strip_prefix("editorial-nightly-")
        .and_then(|name| name.strip_suffix(".warc"))
        .expect("a timestamped WARC filename");
    assert!(timestamp.len() >= 9 && timestamp.bytes().all(|byte| byte.is_ascii_digit()));

    Ok(())
}

#[test]
fn an_archive_pages_each_exposed_collection_after_the_probes()
-> Result<(), Box<dyn std::error::Error>> {
    let server = serve_site(22)?;
    let port = server.port();
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("archives");
    // The collection identifier a session names itself by, which the standard rules hold the
    // file and its request hosts to.
    let session = format!("127.0.0.1-{port}-1787184000");
    let options = archive_options(port, &output, &session);
    let root = format!("http://127.0.0.1:{port}/");
    let page = |endpoint: &str, page: usize| {
        format!(
            "{root}wp-json/wp/v2/{endpoint}?before={BEFORE}&orderby=id&order=asc&page={page}\
                 &per_page=100"
        )
    };

    let outcome = run_archive(
        ArchiveDriver::new(options.base.clone(), before()),
        &options,
        before(),
        true,
    )?;

    assert_eq!(outcome, CommandOutcome::Success);
    let mut expected = vec![
        "/wp-json".to_owned(),
        "/wp-json/wp/v2".to_owned(),
        "/wp-json/wp/v2/types".to_owned(),
        "/wp-json/wp/v2/taxonomies".to_owned(),
        "/wp-json/wp/v2/block-types".to_owned(),
        "/wp-json/wp/v2/block-patterns/categories".to_owned(),
        "/wp-json/wp/v2/block-patterns/patterns".to_owned(),
        "/wp-json/wp/v2/menu-locations".to_owned(),
    ];
    expected.extend(
        Endpoint::ALL
            .iter()
            .map(|endpoint| format!("/wp-json/wp/v2/{endpoint}")),
    );
    // The custom collections are probed after the supported endpoints, in registry order.
    expected.extend(["/wp-json/wp/v2/videos", "/wp-json/wp/v2/series"].map(str::to_owned));
    expected.extend(
        [("pages", 1), ("pages", 2), ("comments", 1), ("videos", 1)]
            .map(|(endpoint, number)| page(endpoint, number)[root.len() - 1..].to_owned()),
    );
    assert_eq!(server.finish(), expected);

    let warc = output.join(format!("{session}.warc"));
    assert!(std::fs::read(&warc)?.starts_with(b"WARC/"));
    let resume = inspect_archive(&warc)?;
    assert_eq!(resume.checkpoint, Checkpoint::Finished);
    assert!(resume.warnings.is_empty());
    assert_eq!(resume.before, Some(before()));
    assert_eq!(
        resume.endpoints[8..],
        [
            custom("videos", Registry::Types),
            custom("series", Registry::Taxonomies),
        ]
    );
    let vias = metadata_vias(&warc)?;
    let seeds = vias.iter().filter(|(_, via)| via.is_none()).count();
    assert_eq!(seeds, 16);
    assert_eq!(
        vias[16..],
        [
            (
                format!("{root}wp-json/wp/v2/videos"),
                Some(format!("{root}wp-json/wp/v2/types"))
            ),
            (
                format!("{root}wp-json/wp/v2/series"),
                Some(format!("{root}wp-json/wp/v2/taxonomies"))
            ),
            (page("pages", 1), Some(format!("{root}wp-json/wp/v2/pages"))),
            (page("pages", 2), Some(page("pages", 1))),
            (
                page("comments", 1),
                Some(format!("{root}wp-json/wp/v2/comments"))
            ),
            (
                page("videos", 1),
                Some(format!("{root}wp-json/wp/v2/videos"))
            ),
        ]
    );

    assert_archive_lint(&warc)?;

    Ok(())
}

#[test]
fn a_resumed_archive_continues_the_endpoint_via_its_last_page()
-> Result<(), Box<dyn std::error::Error>> {
    let server = serve_site(7)?;
    let port = server.port();
    let directory = tempfile::tempdir()?;
    let options = archive_options(port, directory.path(), "site-resumed");
    let root = format!("http://127.0.0.1:{port}/wp-json/wp/v2/");
    let page = |endpoint: &str, page: usize| {
        format!("{root}{endpoint}?before={BEFORE}&orderby=id&order=asc&page={page}&per_page=100")
    };

    let outcome = run_archive(
        ArchiveDriver::resume(
            options.base.clone(),
            before(),
            resumption(Endpoint::Comments, 1, Some(2)),
            vec![custom("videos", Registry::Types)],
        ),
        &options,
        before(),
        true,
    )?;

    assert_eq!(outcome, CommandOutcome::Success);
    let requests = server.finish();
    assert_eq!(requests.len(), 7);
    assert_eq!(
        requests[1..6],
        [
            "/wp-json/wp/v2/users",
            "/wp-json/wp/v2/categories",
            "/wp-json/wp/v2/tags",
            "/wp-json/wp/v2/navigation",
            "/wp-json/wp/v2/videos"
        ]
    );
    assert_eq!(
        metadata_vias(&directory.path().join("site-resumed.warc"))?,
        [
            (page("comments", 2), Some(page("comments", 1))),
            (format!("{root}users"), None),
            (format!("{root}categories"), None),
            (format!("{root}tags"), None),
            (format!("{root}navigation"), None),
            (format!("{root}videos"), Some(format!("{root}types"))),
            (page("videos", 1), Some(format!("{root}videos"))),
        ]
    );
    let resume = inspect_archive(directory.path().join("site-resumed.warc"))?;
    assert_eq!(resume.checkpoint, Checkpoint::Finished);
    assert!(resume.warnings.is_empty());
    assert_eq!(resume.before, Some(before()));
    assert!(
        resume
            .endpoints
            .contains(&custom("videos", Registry::Types))
    );

    Ok(())
}

#[test]
fn a_limited_archive_reports_problems_at_its_checkpoint() -> Result<(), Box<dyn std::error::Error>>
{
    let server = serve_site(19)?;
    let port = server.port();
    let directory = tempfile::tempdir()?;
    let mut options = archive_options(port, directory.path(), "site-limited");
    options.limit = Some(19);

    let outcome = run_archive(
        ArchiveDriver::new(options.base.clone(), before()),
        &options,
        before(),
        true,
    )?;

    assert_eq!(outcome, CommandOutcome::ReportedProblems);
    assert_eq!(server.finish().len(), 19);

    let warc = directory.path().join("site-limited.warc");
    let resume = inspect_archive(&warc)?;
    assert_eq!(
        resume.checkpoint,
        Checkpoint::Resume(resumption(Endpoint::Pages, 1, Some(2)))
    );
    assert_eq!(resume.before, Some(before()));
    assert!(resume.warnings.is_empty());
    assert!(
        resume
            .endpoints
            .contains(&custom("videos", Registry::Types))
    );
    assert!(
        resume
            .endpoints
            .contains(&custom("series", Registry::Taxonomies))
    );

    assert_eq!(
        resume_info(&ResumeInfoOptions { input: warc }, true)?,
        CommandOutcome::ReportedProblems
    );

    Ok(())
}

#[test]
fn resumed_progress_uses_endpoint_specific_page_sizes() -> Result<(), Box<dyn std::error::Error>> {
    let server = serve_site(18)?;
    let port = server.port();
    let directory = tempfile::tempdir()?;
    let mut options = archive_options(port, directory.path(), "site-page-sizes");
    options.limit = Some(18);
    options.per_page = PerPageOptions {
        values: vec![parse_per_page("videos:1").expect("a page-size setting")],
    };

    let driver = options
        .per_page
        .configure(ArchiveDriver::new(options.base.clone(), before()));
    assert_eq!(
        run_archive(driver, &options, before(), true)?,
        CommandOutcome::ReportedProblems
    );
    assert_eq!(server.finish().len(), 18);

    let warc = directory.path().join("site-page-sizes.warc");
    let default = inspect_archive(&warc)?;
    let configured =
        inspect_archive_with_config(&warc, |driver| options.per_page.configure(driver))?;
    let video_pages = |info: &archivindex_wordpress_scraper::resume::ResumeInfo| {
        info.probes
            .iter()
            .find(|probe| probe.collection.name() == "videos")
            .and_then(|probe| probe.total_pages)
    };

    assert_eq!(video_pages(&default), Some(1));
    assert_eq!(video_pages(&configured), Some(3));

    Ok(())
}

#[test]
fn resume_archive_reads_prior_segments_and_does_not_reprobe()
-> Result<(), Box<dyn std::error::Error>> {
    let server = serve_site(22)?;
    let port = server.port();
    let directory = tempfile::tempdir()?;
    let mut initial = archive_options(port, directory.path(), "site-chain-100000000");
    initial.limit = Some(18);

    assert_eq!(
        super::run_archive_for_session(
            ArchiveDriver::new(initial.base.clone(), before()),
            &initial,
            before(),
            true,
            "site-chain",
        )?,
        CommandOutcome::ReportedProblems
    );
    let initial_warc = directory.path().join("site-chain-100000000.warc");
    let mut encoder = GzEncoder::new(
        std::fs::File::create(directory.path().join("site-chain-100000000.warc.gz"))?,
        Compression::default(),
    );
    encoder.write_all(&std::fs::read(&initial_warc)?)?;
    encoder.finish()?;
    std::fs::remove_file(initial_warc)?;
    assert_eq!(
        resume_archive(
            &ResumeArchiveOptions {
                config: None,
                output: directory.path().to_owned(),
                session_name: "site-chain".to_owned(),
                before: Some(before()),
                revisit_index: None,
                limit: None,
                per_page: PerPageOptions::default(),
                cookie: None,
            },
            true,
        )?,
        CommandOutcome::Success
    );

    let requests = server.finish();
    assert_eq!(requests.len(), 22);
    assert!(requests[18].contains("/pages?") && requests[18].contains("page=1"));
    assert!(requests[19].contains("/pages?") && requests[19].contains("page=2"));
    assert!(requests[20].contains("/comments?") && requests[20].contains("page=1"));
    assert!(requests[21].contains("/videos?") && requests[21].contains("page=1"));
    assert!(requests[18..].iter().all(|request| request.contains('?')));

    let segments = super::SessionInventory::read(directory.path())?.warcs("site-chain")?;
    assert_eq!(segments.len(), 2);
    assert_eq!(
        metadata_vias(&segments[1])?
            .into_iter()
            .map(|(url, _)| url)
            .collect::<Vec<_>>(),
        requests[18..]
            .iter()
            .map(|request| format!("http://127.0.0.1:{port}{request}"))
            .collect::<Vec<_>>()
    );

    Ok(())
}

#[test]
fn resume_info_warns_and_rolls_back_an_incomplete_capture() -> Result<(), Box<dyn std::error::Error>>
{
    let server = serve_site(19)?;
    let port = server.port();
    let directory = tempfile::tempdir()?;
    let mut options = archive_options(port, directory.path(), "site-damaged");
    options.limit = Some(19);
    let _ = run_archive(
        ArchiveDriver::new(options.base.clone(), before()),
        &options,
        before(),
        true,
    )?;
    assert_eq!(server.finish().len(), 19);

    let source = directory.path().join("site-damaged.warc");
    let damaged = directory.path().join("request-only.warc");
    let mut records = WarcReader::from_path(&source)?
        .iter_records::<NoExtension>()
        .records()
        .collect::<Result<Vec<_>, _>>()?;
    let last_request = records
        .iter()
        .rposition(|record| matches!(record, Record::Request { .. }))
        .expect("a request record");
    records.truncate(last_request + 1);
    let mut bytes = Vec::new();
    let mut writer = WarcWriter::new(&mut bytes);
    for record in records {
        writer.write(&record.into_raw()?)?;
    }
    writer.flush()?;
    std::fs::write(&damaged, bytes)?;

    let resume = inspect_archive(&damaged)?;
    assert_eq!(
        resume.checkpoint,
        Checkpoint::Resume(resumption(Endpoint::Pages, 0, None))
    );
    assert_eq!(resume.before, Some(before()));
    assert!(resume.warnings.iter().any(|warning| {
        warning.contains("missing response or revisit and metadata") && warning.contains("pages")
    }));
    assert!(
        resume
            .endpoints
            .contains(&custom("videos", Registry::Types))
    );
    assert!(
        resume
            .endpoints
            .contains(&custom("series", Registry::Taxonomies))
    );

    Ok(())
}

#[test]
fn a_failure_during_the_initial_requests_is_fatal() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("capture.toml");
    std::fs::write(&config, "[session.retry]\nattempts = 1\n")?;
    let mut options = archive_options(dead_port()?, directory.path(), "site-unreachable");
    options.config = Some(config);

    let result = run_archive(
        ArchiveDriver::new(options.base.clone(), before()),
        &options,
        before(),
        true,
    );

    assert!(matches!(
        result,
        Err(Error::InitialRequestsIncomplete(output))
            if output == directory.path().join("site-unreachable.warc")
    ));

    Ok(())
}

/// Every metadata record's target and `via`, in WARC order.
fn metadata_vias(warc: &Path) -> Result<Vec<MetadataVia>, Box<dyn std::error::Error>> {
    let mut metadata = Vec::new();
    let gzip = std::fs::read(warc)?
        .get(..2)
        .is_some_and(|magic| magic == [0x1f, 0x8b]);
    let reader = if gzip {
        WarcReader::from_path_gzip(warc)?
    } else {
        WarcReader::from_path(warc)?
    };
    for record in reader.iter_records::<NoExtension>().records() {
        let Record::Metadata { header, body } = record? else {
            continue;
        };
        let Some(target) = header.target_uri else {
            continue;
        };
        let FieldsBlock::Fields(fields) = body else {
            continue;
        };
        metadata.push((target.into_string(), fields.via().map(str::to_owned)));
    }

    Ok(metadata)
}

#[test]
fn changed_page_totals_report_successive_differences() {
    let coverage = CommentCompleteness {
        total_pages: Some(4),
        advertised_page_totals: vec![2, 4, 3],
        captured_pages: vec![1, 2, 3],
    };

    assert_eq!(
        page_total_change_warning(&coverage).as_deref(),
        Some(
            "X-WP-TotalPages changed over the WARC session (2 -> 4 -> 3); successive \
                 differences: +2, -1"
        )
    );
}
