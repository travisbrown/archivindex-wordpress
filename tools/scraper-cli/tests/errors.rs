//! CLI diagnostics retain each layer of an operational error.

use std::process::{Command, Output};

use archivindex_warc::io::write::WarcWriter;
use archivindex_warc::record::Record;
use jiff::Timestamp;

/// The shared subscriber receives this CLI's tracing events and applies verbosity filters.
#[test]
fn tracing_diagnostics_follow_the_shared_verbosity_setting()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("comments.warc");
    let record: Record = Record::request("https://example.com/wp-json", Timestamp::now())?
        .body(b"GET /wp-json HTTP/1.1\r\nhost: example.com\r\n\r\n".to_vec())?;
    let mut writer = WarcWriter::from_path(&input)?;
    writer.write(&record.into_raw()?)?;
    writer.flush()?;
    for (command, message, counts) in [
        ("lint", "core lints passed:", [1, 0, 0]),
        (
            "check-comments",
            "has no qualifying WordPress comments capture",
            [1, 1, 0],
        ),
    ] {
        for (flags, expected) in [vec!["-v"], vec![], vec!["--quiet"]]
            .into_iter()
            .zip(counts)
        {
            let output = Command::new(env!("CARGO_BIN_EXE_archivindex-wordpress-scraper"))
                .args([command, "--input"])
                .arg(&input)
                .args(flags)
                .output()?;
            let stderr = String::from_utf8(output.stderr)?;
            assert_eq!(output.status.code(), Some(1), "{stderr}");
            assert_eq!(stderr.matches(message).count(), expected, "{stderr}");
        }
    }
    Ok(())
}

fn diagnostic(output: Output) -> String {
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    String::from_utf8(output.stderr).expect("UTF-8 diagnostics")
}

/// The shared reporter must retain the parser's cause when the configuration error only names
/// the file that failed to load.
#[test]
fn invalid_configuration_reports_its_cause_once() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("invalid.json");
    std::fs::write(&config, "{")?;

    let report = diagnostic(
        Command::new(env!("CARGO_BIN_EXE_archivindex-wordpress-scraper"))
            .args(["archive", "--base", "example.com", "--output"])
            .arg(directory.path())
            .arg("--config")
            .arg(&config)
            .output()?,
    );

    assert!(
        report.contains("cannot parse configuration file"),
        "{report}"
    );
    assert_eq!(report.matches("invalid.json").count(), 1, "{report}");
    assert_eq!(
        report.matches("EOF while parsing an object").count(),
        1,
        "{report}"
    );
    Ok(())
}

/// Errors wrapping filesystem failures must name the operation and include the operating
/// system's cause once, including through the scraper library's error wrappers.
#[test]
fn missing_inputs_report_each_error_layer_once() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("missing.warc");
    let cause = std::fs::File::open(&missing).unwrap_err().to_string();

    for (command, context) in [
        ("lint", "WordPress archive lint error"),
        ("resume-info", "WordPress archive resume inspection error"),
    ] {
        let report = diagnostic(
            Command::new(env!("CARGO_BIN_EXE_archivindex-wordpress-scraper"))
                .args([command, "--input"])
                .arg(&missing)
                .output()?,
        );

        assert_eq!(report.matches(context).count(), 1, "{report}");
        assert_eq!(report.matches("missing.warc").count(), 1, "{report}");
        assert_eq!(report.matches(&cause).count(), 1, "{report}");
    }
    Ok(())
}
