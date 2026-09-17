//! A command-line front end for capturing and reading `WordPress` REST API resources.

mod combine;
mod sessions;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use archivindex_archiver::capture::{CaptureSummary, ProgressControl, ProgressEvent};
use archivindex_archiver::session::{
    Capture, Driver, Inspection, Request, Session, SessionSummary,
};
use archivindex_archiver::{Archiver, Config};
use archivindex_cli_support::progress::{bar, spinner};
use archivindex_cli_support::{CommandOutcome, Verbosity, config, exit_code, interrupt_flag};
use archivindex_warc_ops::lint::Severity;
use archivindex_wordpress_scraper::archive::{
    ArchiveDriver, Checkpoint, DEFAULT_PER_PAGE, PaginationProgress, Site,
};
use archivindex_wordpress_scraper::complete::{
    CommentCompletionSummary, complete_comments_with_delay,
};
use archivindex_wordpress_scraper::lint::lint_archive;
use archivindex_wordpress_scraper::read::{
    CommentCompleteness, CommentUpdateAnchor, check_comment_collections,
    find_comment_update_anchors, read_comments,
};
use archivindex_wordpress_scraper::resume::{
    inspect_archive, inspect_archive_with_config, inspect_archive_with_restored_probes_and_config,
};
use archivindex_wordpress_scraper::{CommentDriver, CommentProgress};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar};

use crate::combine::{CombineOptions, combine_archives};
use crate::sessions::{SegmentName, SessionInventory};

fn main() -> ExitCode {
    let opts = Opts::parse();
    opts.verbosity.init_logging();

    exit_code(run(opts))
}

fn run(opts: Opts) -> Result<CommandOutcome, Error> {
    let quiet = opts.verbosity.is_quiet();

    match opts.command {
        Command::Archive(options) => archive_site(&options, quiet),
        Command::Check(options) => check_wp_comments(&options, quiet),
        Command::Combine(options) => combine_wp_archives(&options, quiet),
        Command::Complete(options) => complete_wp_comments(&options, quiet),
        Command::Lint(options) => lint_wp_archive(&options, quiet),
        Command::Read(options) => read_wp_comments(options),
        Command::ResumeArchive(options) => resume_archive(&options, quiet),
        Command::ResumeInfo(options) => resume_info(&options, quiet),
        Command::Update(options) => update_comments(&options, quiet),
    }
}

/// Combine a site's archive and resume-run segments into one WARC.
fn combine_wp_archives(options: &CombineOptions, quiet: bool) -> Result<CommandOutcome, Error> {
    let summary = combine_archives(options)?;
    if !quiet {
        println!(
            "Combined {} records from {} files for {} into {}",
            summary.records,
            summary.files,
            options.domain,
            options.output.display()
        );
    }

    Ok(CommandOutcome::Success)
}

/// Validate the capture graph and collection pagination protocol of an archive WARC.
fn lint_wp_archive(options: &LintOptions, quiet: bool) -> Result<CommandOutcome, Error> {
    let report = lint_archive(&options.input)?;
    log::info!("core lints passed: {}", report.core_lints_passed);
    for finding in &report.findings {
        let subject = finding.subject.as_ref().map_or_else(
            || "the file".to_owned(),
            |subject| format!("record {}", subject.index),
        );
        match finding.violation.severity() {
            Severity::Error => log::error!("{subject}: {}", finding.violation),
            Severity::Warning => log::warn!("{subject}: {}", finding.violation),
        }
    }

    if !quiet {
        for pagination in &report.pagination {
            let pages = pagination
                .pages
                .map_or_else(|| "unknown".to_owned(), |pages| pages.to_string());
            let items = pagination
                .items
                .map_or_else(|| "unknown".to_owned(), |items| items.to_string());
            println!("{}: {pages} pages, {items} items", pagination.endpoint);
        }
        println!(
            "{}: {} roots, {} known probes, {} custom probes, {} paginated endpoints; {} errors, {} warnings",
            options.input.display(),
            report.roots,
            report.known_probes,
            report.custom_probes,
            report.pagination.len(),
            report.error_count(),
            report.warning_count(),
        );
    }

    Ok(CommandOutcome::from_reported_problems(!report.is_clean()))
}

/// Archive every supported collection a site exposes, beginning with the API roots and probes.
fn archive_site(options: &ArchiveRunOptions, quiet: bool) -> Result<CommandOutcome, Error> {
    let before = Utc::now();
    let session = options
        .session_name
        .clone()
        .unwrap_or_else(|| default_session_prefix(&options.base));
    let mut run = options.clone();
    std::fs::create_dir_all(&options.output)?;
    let inventory = SessionInventory::read(&options.output)?;
    run.session_name = Some(inventory.next_name(&session)?);

    run_archive_for_session(
        run.per_page
            .configure(ArchiveDriver::new(run.base.clone(), before)),
        &run,
        before,
        quiet,
        &session,
    )
}

/// Continue an archive from the ordered WARC segments already written for its session.
fn resume_archive(options: &ResumeArchiveOptions, quiet: bool) -> Result<CommandOutcome, Error> {
    let inventory = SessionInventory::read(&options.output)?;
    let paths = inventory.warcs(&options.session_name)?;
    let first_path = paths
        .first()
        .expect("session discovery returns at least one path");
    let first =
        inspect_archive_with_config(first_path, |driver| options.per_page.configure(driver))?;
    report_resume_warnings(first_path, &first.warnings);
    if first.probes.len() != first.endpoints.len()
        || first
            .endpoints
            .iter()
            .zip(&first.probes)
            .any(|(endpoint, probe)| endpoint.name() != probe.collection.name())
    {
        return Err(Error::MissingSessionProbes(first_path.clone()));
    }
    let before = first
        .before
        .or(options.before)
        .ok_or_else(|| Error::MissingResumeCutoff(first_path.clone()))?;

    let mut latest = first.clone();
    for path in paths.iter().skip(1) {
        let info = inspect_archive_with_restored_probes_and_config(
            path,
            &first.probes,
            Some(before),
            |driver| options.per_page.configure(driver),
        )?;
        report_resume_warnings(path, &info.warnings);
        if info.site != first.site {
            return Err(Error::SessionSiteMismatch {
                path: path.clone(),
                expected: first.site.base().to_owned(),
                actual: info.site.base().to_owned(),
            });
        }
        if info.before.is_some_and(|candidate| candidate != before) {
            return Err(Error::SessionCutoffMismatch {
                path: path.clone(),
                expected: before,
                actual: info.before.expect("the condition requires a cutoff"),
            });
        }
        latest = info;
    }

    let mut resumption = match latest.checkpoint {
        Checkpoint::Resume(resumption) => resumption,
        checkpoint => {
            return match checkpoint {
                Checkpoint::Finished => {
                    if !quiet {
                        println!("{} is complete", options.session_name);
                    }
                    Ok(CommandOutcome::Success)
                }
                Checkpoint::Initial => Err(Error::InitialArchiveCannotResume(first_path.clone())),
                Checkpoint::Resume(_) => unreachable!("the outer match excludes this variant"),
            };
        }
    };
    resumption.endpoint = first
        .probes
        .iter()
        .find(|probe| probe.collection.name() == resumption.endpoint.name())
        .map(|probe| probe.collection.clone())
        .ok_or_else(|| Error::UnknownEndpoint(resumption.endpoint.name().to_owned()))?;
    let driver = options
        .per_page
        .configure(ArchiveDriver::resume_with_probes(
            first.site.clone(),
            before,
            resumption,
            first.probes,
        ));
    let run = ArchiveRunOptions {
        config: options.config.clone(),
        base: first.site,
        output: options.output.clone(),
        session_name: Some(inventory.next_name(&options.session_name)?),
        revisit_index: options.revisit_index.clone(),
        limit: options.limit,
        per_page: options.per_page.clone(),
        cookie: options.cookie.clone(),
    };

    run_archive_for_session(driver, &run, before, quiet, &options.session_name)
}

/// Recover and print the command continuing a collection archive WARC.
fn resume_info(options: &ResumeInfoOptions, quiet: bool) -> Result<CommandOutcome, Error> {
    let info = inspect_archive(&options.input)?;
    for warning in &info.warnings {
        log::warn!("{warning}");
    }

    match info.checkpoint {
        Checkpoint::Finished => {
            if !quiet {
                println!("{} is complete", options.input.display());
            }
            if info.warnings.is_empty() {
                Ok(CommandOutcome::Success)
            } else {
                Ok(CommandOutcome::ReportedProblems)
            }
        }
        Checkpoint::Resume(_) => {
            let _ = info
                .before
                .ok_or_else(|| Error::MissingResumeCutoff(options.input.clone()))?;
            let parent = options
                .input
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let session = session_name_from_warc(&options.input)
                .ok_or_else(|| Error::SessionName(options.input.clone()))?;
            println!(
                "{}",
                resume_command(
                    parent,
                    &session,
                    None,
                    None,
                    None,
                    &PerPageOptions::default(),
                )
            );
            Ok(CommandOutcome::ReportedProblems)
        }
        Checkpoint::Initial => Err(Error::InitialArchiveCannotResume(options.input.clone())),
    }
}

/// Run an archiving session to a new plain WARC in the output directory.
///
/// A run that stops after the initial requests reports the command that continues it; one that
/// stops during them is an error, since there is nothing to continue.
#[cfg(test)]
fn run_archive(
    driver: ArchiveDriver,
    options: &ArchiveRunOptions,
    before: DateTime<Utc>,
    quiet: bool,
) -> Result<CommandOutcome, Error> {
    let archive_session = options
        .session_name
        .clone()
        .unwrap_or_else(|| default_session_prefix(&options.base));

    run_archive_for_session(driver, options, before, quiet, &archive_session)
}

/// Run one segment of an archive whose complete history is identified by `archive_session`.
fn run_archive_for_session(
    driver: ArchiveDriver,
    options: &ArchiveRunOptions,
    before: DateTime<Utc>,
    quiet: bool,
    archive_session: &str,
) -> Result<CommandOutcome, Error> {
    let session_name = options
        .session_name
        .clone()
        .unwrap_or_else(|| options.base.session_name(Utc::now()));
    std::fs::create_dir_all(&options.output)?;
    let output = options.output.join(format!("{session_name}.warc"));
    // The directory accumulates one plain WARC per run, to be merged later.
    let mut config: Config = config::load(options.config.as_deref())?;
    config.gzip_warc = false;
    let mut archiver = Archiver::new(config)?;
    if let Some(cookie) = &options.cookie {
        archiver = archiver.cookie_for(options.base.root().as_str(), cookie)?;
    }

    let mut state = ArchiveRunState::new(driver);
    // An interrupt ends the session cleanly, so its captures are published and the checkpoint
    // reported instead of abandoning a partial file.
    let interrupted = interrupt_flag();
    let mut session = Session::new(archiver, &session_name, &mut state, &output)?.progress(
        move |event: ProgressEvent<'_>| {
            match event {
                // Once a response has been captured, let inspection and recording finish before
                // honoring an interrupt. The session then publishes its recorded progress.
                ProgressEvent::Captured { .. } => ProgressControl::Continue,
                ProgressEvent::Written { .. }
                | ProgressEvent::Started { .. }
                | ProgressEvent::Retrying { .. }
                | ProgressEvent::Failed { .. } => {
                    if interrupted.load(Ordering::Relaxed) {
                        ProgressControl::Cancel
                    } else {
                        ProgressControl::Continue
                    }
                }
            }
        },
    );

    if let Some(revisit_index) = &options.revisit_index {
        session = session.revisit_index(revisit_index);
    }
    if let Some(limit) = options.limit {
        session = session.limit(limit);
    }

    let summary = session.run()?;
    state.progress.finish();
    report_archive_problems(&summary, &state.driver, options);
    let checkpoint = state.recorded;

    match checkpoint {
        Checkpoint::Finished if summary.is_complete() => {
            if !quiet {
                println!(
                    "Archived {} captures from {} to {}",
                    summary.seed_captures.len() + summary.extra_captures.len(),
                    options.base.base(),
                    output.display()
                );
            }
            Ok(CommandOutcome::Success)
        }
        Checkpoint::Finished => {
            log::warn!(
                "the archive session reported problems after its final driver checkpoint; \
                 start a new archive to guarantee completeness"
            );
            Ok(CommandOutcome::ReportedProblems)
        }
        Checkpoint::Resume(_) => {
            log::warn!("a partial archive was published at {}", output.display());
            println!(
                "Continue the archive with: {}",
                resume_command(
                    &options.output,
                    archive_session,
                    options.config.as_deref(),
                    options.revisit_index.as_deref(),
                    (!summary_has_pagination(&summary)).then_some(before),
                    &options.per_page,
                )
            );
            Ok(CommandOutcome::ReportedProblems)
        }
        Checkpoint::Initial => Err(Error::InitialRequestsIncomplete(output)),
    }
}

/// Probe spinner followed by one progress bar per exposed collection with a reported page count.
struct ArchiveProgress {
    multi: MultiProgress,
    probing: ProgressBar,
    pagination: BTreeMap<String, ProgressBar>,
}

impl ArchiveProgress {
    fn new(driver: &ArchiveDriver) -> Self {
        let multi = MultiProgress::new();
        let probing = multi.add(spinner(driver.to_string(), None));
        let mut progress = Self {
            multi,
            probing,
            pagination: BTreeMap::new(),
        };
        let initial = driver.pagination_progress();
        progress.add_bars(&initial);
        progress.update_bars(initial);

        progress
    }

    fn update(&mut self, driver: &ArchiveDriver) {
        let progress = driver.pagination_progress();
        if driver.probes_finished() {
            self.probing.finish_and_clear();
            self.add_bars(&progress);
        } else {
            self.probing.set_message(driver.to_string());
        }
        self.update_bars(progress);
    }

    fn add_bars(&mut self, progress: &[PaginationProgress]) {
        for endpoint in progress {
            if !self.pagination.contains_key(endpoint.collection.name()) {
                let length = u64::try_from(endpoint.total_pages).unwrap_or(u64::MAX);
                // The message is set by `update_bars` as soon as the collection reports progress.
                let collection_bar = self.multi.add(bar(length, "", Some("pages")));
                self.pagination
                    .insert(endpoint.collection.name().to_owned(), collection_bar);
            }
        }
    }

    fn update_bars(&self, progress: impl IntoIterator<Item = PaginationProgress>) {
        for endpoint in progress {
            if let Some(bar) = self.pagination.get(endpoint.collection.name()) {
                bar.set_message(endpoint.collection.to_string());
                bar.set_position(u64::try_from(endpoint.page).unwrap_or(u64::MAX));
            }
        }
    }

    fn finish(&self) {
        self.probing.finish_and_clear();
        for bar in self.pagination.values() {
            bar.finish_and_clear();
        }
    }
}

fn report_archive_problems(
    summary: &SessionSummary,
    driver: &ArchiveDriver,
    options: &ArchiveRunOptions,
) {
    for failure in &summary.failures {
        log::warn!("Failed to capture {}: {}", failure.url, failure.error);
    }
    if let Some(error) = &summary.fatal_error {
        log::warn!("The session ended early: {error}");
    }
    if summary.cancelled {
        log::warn!("the session was cancelled before all requested captures were completed");
    }
    if summary.partial_captures() > 0 {
        log::warn!(
            "{} capture(s) were unexpectedly truncated",
            summary.partial_captures()
        );
    }
    for (endpoint, status) in driver.probed() {
        let status = *status;
        if status == 404 {
            log::info!("{} does not expose {endpoint}", options.base.base());
        } else if !(200..300).contains(&status) && status != 304 {
            log::warn!(
                "{} answered the {endpoint} probe with status {status}; the endpoint was skipped",
                options.base.base()
            );
        }
    }
}

fn summary_has_pagination(summary: &SessionSummary) -> bool {
    summary
        .seed_captures
        .iter()
        .chain(&summary.extra_captures)
        .any(|capture| {
            url::Url::parse(&capture.url).is_ok_and(|url| {
                let mut page = false;
                let mut before = false;
                for (name, _) in url.query_pairs() {
                    page |= name == "page";
                    before |= name == "before";
                }
                page && before
            })
        })
}

fn report_resume_warnings(path: &Path, warnings: &[String]) {
    for warning in warnings {
        log::warn!("{}: {warning}", path.display());
    }
}

/// The stable default prefix shared by every run segment for a site.
fn default_session_prefix(site: &Site) -> String {
    site.session_name(DateTime::<Utc>::UNIX_EPOCH)
        .strip_suffix("-0")
        .expect("the epoch session name ends in -0")
        .to_owned()
}

fn session_name_from_warc(path: &Path) -> Option<String> {
    SegmentName::parse(path).map(|name| name.session)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PerPageValue {
    endpoint: Option<String>,
    value: usize,
}

fn parse_per_page(value: &str) -> Result<PerPageValue, String> {
    let (endpoint, value) = value
        .split_once(':')
        .map_or((None, value), |(endpoint, value)| (Some(endpoint), value));
    if endpoint
        .is_some_and(|endpoint| endpoint.is_empty() || endpoint.chars().any(char::is_whitespace))
    {
        return Err("endpoint name must be non-empty and contain no whitespace".to_owned());
    }
    let per_page = value
        .parse::<usize>()
        .map_err(|_| "page size must be an integer from 1 through 100".to_owned())?;
    let value = (1..=DEFAULT_PER_PAGE)
        .contains(&per_page)
        .then_some(per_page)
        .ok_or_else(|| "page size must be from 1 through 100".to_owned())?;

    Ok(PerPageValue {
        endpoint: endpoint.map(str::to_owned),
        value,
    })
}

/// Default and named page sizes supplied by repeatable `--per-page` options.
#[derive(Clone, Debug, Default, clap::Args)]
struct PerPageOptions {
    /// Items per page (1–100), globally (`20`) or for one endpoint (`media:2`). May be repeated;
    /// the last setting for each scope wins. The default is 100.
    #[arg(long = "per-page",
        value_name = "COUNT|ENDPOINT:COUNT",
        value_parser = parse_per_page,
        action = clap::ArgAction::Append
    )]
    values: Vec<PerPageValue>,
}

impl PerPageOptions {
    fn default_value(&self) -> usize {
        self.values
            .iter()
            .rev()
            .find_map(|setting| setting.endpoint.is_none().then_some(setting.value))
            .unwrap_or(DEFAULT_PER_PAGE)
    }

    fn endpoint_values(&self) -> BTreeMap<&str, usize> {
        self.values
            .iter()
            .filter_map(|setting| {
                setting
                    .endpoint
                    .as_deref()
                    .map(|endpoint| (endpoint, setting.value))
            })
            .collect()
    }

    fn configure(&self, driver: ArchiveDriver) -> ArchiveDriver {
        self.endpoint_values().into_iter().fold(
            driver.with_per_page(self.default_value()),
            |driver, (endpoint, value)| driver.with_per_page_for(endpoint, value),
        )
    }
}

/// The command continuing the archive segments sharing `session_name`.
///
/// A cookie is not repeated because it is a secret. The capture limit is not repeated so a
/// continuation finishes by default.
fn resume_command(
    output: &Path,
    session_name: &str,
    config: Option<&Path>,
    revisit_index: Option<&Path>,
    before: Option<DateTime<Utc>>,
    per_page: &PerPageOptions,
) -> String {
    let mut command = format!(
        "archivindex-wordpress-scraper resume-archive --output {} --session-name {}",
        shell_word(&output.to_string_lossy()),
        shell_word(session_name),
    );
    for (flag, path) in [("--config", config), ("--revisit-index", revisit_index)] {
        if let Some(path) = path {
            command.push(' ');
            command.push_str(flag);
            command.push(' ');
            command.push_str(&shell_word(&path.to_string_lossy()));
        }
    }
    if let Some(before) = before {
        command.push_str(" --before ");
        command.push_str(&shell_word(
            &before.to_rfc3339_opts(SecondsFormat::Secs, true),
        ));
    }
    let default_per_page = per_page.default_value();
    if default_per_page != DEFAULT_PER_PAGE {
        write!(command, " --per-page {default_per_page}").expect("writing to a String cannot fail");
    }
    for (endpoint, value) in per_page.endpoint_values() {
        write!(command, " --per-page {}:{value}", shell_word(endpoint))
            .expect("writing to a String cannot fail");
    }

    command
}

/// Quote one command-line argument for a POSIX-compatible shell.
fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

/// Own traversal, recorded progress, and its display for one archive run. The checkpoint is usable
/// only after the session successfully publishes its archive.
struct ArchiveRunState {
    driver: ArchiveDriver,
    progress: ArchiveProgress,
    recorded: Checkpoint,
    pending: Option<Checkpoint>,
    // Once recording leaves a gap, later captures cannot advance the resume checkpoint.
    incomplete: bool,
}

impl ArchiveRunState {
    fn new(driver: ArchiveDriver) -> Self {
        Self {
            progress: ArchiveProgress::new(&driver),
            recorded: driver.checkpoint(),
            driver,
            pending: None,
            incomplete: false,
        }
    }
}

impl Driver for ArchiveRunState {
    fn next(&mut self) -> Option<Request> {
        self.driver.next()
    }

    fn inspect(&mut self, capture: &Capture<'_>) -> Inspection {
        let before = self.driver.checkpoint();
        let inspection = self.driver.inspect(capture);
        self.pending = Some(before);
        inspection
    }

    fn recorded(&mut self, capture: Option<&CaptureSummary>) {
        if let Some(before) = self.pending.take()
            && !self.incomplete
        {
            match capture {
                Some(capture) if !capture.is_partial() => self.recorded = self.driver.checkpoint(),
                Some(_) => {
                    self.recorded = before;
                    self.incomplete = true;
                }
                None => self.incomplete = true,
            }
        }
        self.progress.update(&self.driver);
    }

    fn failed(&mut self, url: &str, error: &archivindex_archiver::Error) {
        self.driver.failed(url, error);
    }
}

/// Capture comments from a window that overlaps each site's latest archived comment time.
fn update_comments(options: &UpdateCommentsOptions, quiet: bool) -> Result<CommandOutcome, Error> {
    let updates = comment_update_inputs(&options.input)?;
    let before = Utc::now();
    let overlap = chrono::Duration::from_std(options.overlap)
        .map_err(|_| Error::OverlapOutOfRange(options.overlap))?;
    let runs = updates
        .into_iter()
        .map(|update| {
            let after = update
                .anchor
                .datetime
                .checked_sub_signed(overlap)
                .ok_or(Error::OverlapOutOfRange(options.overlap))?;
            if after >= before {
                return Err(Error::InvalidUpdateWindow { after, before });
            }
            log::info!(
                "updating {} comments from {} after {} and before {} (anchor from {})",
                update.anchor.base_url,
                update.path.display(),
                after.to_rfc3339(),
                before.to_rfc3339(),
                if update.anchor.from_comment {
                    "latest comment"
                } else {
                    "archived before cutoff"
                }
            );
            let driver = CommentDriver::for_window(&update.anchor.base_url, after, before)?;

            Ok(CommentRun {
                site_url: update.anchor.base_url,
                driver,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    capture_comment_run(
        runs,
        CommentRunOptions {
            config: options.config.as_deref(),
            cookie: options.cookie.as_deref(),
            output: &options.output,
            session_name: &options.session_name,
            revisit_index: options.revisit_index.as_deref(),
            limit: options.limit,
            second_sweep: options.second_sweep,
        },
        quiet,
    )
}

struct CommentUpdateInput {
    path: PathBuf,
    anchor: CommentUpdateAnchor,
}

/// Read one update WARC, or every directly contained WARC when `input` is a directory.
fn comment_update_inputs(input: &Path) -> Result<Vec<CommentUpdateInput>, Error> {
    let metadata = std::fs::metadata(input).map_err(|source| Error::UpdateInputRead {
        path: input.to_owned(),
        source,
    })?;
    let mut paths = if metadata.is_dir() {
        let mut paths = Vec::new();
        let entries = std::fs::read_dir(input).map_err(|source| Error::UpdateInputRead {
            path: input.to_owned(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::UpdateInputRead {
                path: input.to_owned(),
                source,
            })?;
            let file_type = entry.file_type().map_err(|source| Error::UpdateInputRead {
                path: entry.path(),
                source,
            })?;
            if file_type.is_file() && is_warc_file_name(&entry.file_name()) {
                paths.push(entry.path());
            }
        }
        if paths.is_empty() {
            return Err(Error::NoUpdateWarcs(input.to_owned()));
        }
        paths
    } else {
        vec![input.to_owned()]
    };
    // Make the input order deterministic before the semantic domain sort below resolves it.
    paths.sort();

    let mut updates_by_site: BTreeMap<String, CommentUpdateInput> = BTreeMap::new();
    for path in paths {
        let anchors = find_comment_update_anchors(&path).map_err(|source| Error::UpdateAnchor {
            path: path.clone(),
            source: Box::new(source),
        })?;
        for anchor in anchors {
            let replace = updates_by_site
                .get(&anchor.base_url)
                .is_none_or(|current| update_anchor_is_newer(&anchor, &current.anchor));
            if replace {
                updates_by_site.insert(
                    anchor.base_url.clone(),
                    CommentUpdateInput {
                        path: path.clone(),
                        anchor,
                    },
                );
            }
        }
    }
    let mut updates = updates_by_site.into_values().collect::<Vec<_>>();
    updates.sort_by(|left, right| {
        update_domain(&left.anchor)
            .cmp(&update_domain(&right.anchor))
            .then_with(|| left.anchor.base_url.cmp(&right.anchor.base_url))
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(updates)
}

/// Prefer actual comment datetimes over URL cutoffs, then retain the greatest datetime.
fn update_anchor_is_newer(candidate: &CommentUpdateAnchor, current: &CommentUpdateAnchor) -> bool {
    (candidate.from_comment && !current.from_comment)
        || (candidate.from_comment == current.from_comment && candidate.datetime > current.datetime)
}

fn is_warc_file_name(name: &OsStr) -> bool {
    let path = Path::new(name);
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("warc"))
        || (path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
            && path
                .file_stem()
                .and_then(|stem| Path::new(stem).extension())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("warc")))
}

fn update_domain(anchor: &CommentUpdateAnchor) -> String {
    url::Url::parse(&anchor.base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_else(|| anchor.base_url.to_ascii_lowercase())
}

struct CommentRun {
    /// Names the site in progress messages and scopes the cookie to its host.
    site_url: String,
    driver: CommentDriver,
}

#[derive(Clone, Copy)]
struct CommentRunOptions<'a> {
    config: Option<&'a Path>,
    cookie: Option<&'a str>,
    output: &'a Path,
    session_name: &'a str,
    revisit_index: Option<&'a Path>,
    limit: Option<usize>,
    second_sweep: bool,
}

fn capture_comment_run(
    runs: Vec<CommentRun>,
    options: CommentRunOptions<'_>,
    quiet: bool,
) -> Result<CommandOutcome, Error> {
    let (site_urls, drivers): (Vec<_>, Vec<_>) = runs
        .into_iter()
        .map(|run| (run.site_url, run.driver.second_sweep(options.second_sweep)))
        .unzip();
    let comment_progress = Rc::new(RefCell::new(CommentRunProgress {
        site_urls: site_urls.clone(),
        snapshots: vec![None; site_urls.len()],
        latest: None,
    }));
    let driver = ProgressingCommentDriver {
        drivers,
        active: None,
        progress: Rc::clone(&comment_progress),
    };
    let config = load_config_for_output(options.config, options.output)?;
    let mut archiver = Archiver::new(config)?;
    if let Some(cookie) = options.cookie {
        for site_url in &site_urls {
            archiver = archiver.cookie_for(site_url, cookie)?;
        }
    }
    let progress = spinner("Downloading comments", None);
    let event_progress = progress.clone();
    let event_comment_progress = Rc::clone(&comment_progress);
    // An interrupt ends the session cleanly, so its captures are published and the pages it had yet
    // to request are reported instead of abandoning a partial file.
    let interrupted = interrupt_flag();
    let mut session = Session::new(archiver, options.session_name, driver, options.output)?
        .progress(move |event: ProgressEvent<'_>| {
            if interrupted.load(Ordering::Relaxed) {
                return ProgressControl::Cancel;
            }
            if matches!(event, ProgressEvent::Written { .. })
                && let Some(message) = event_comment_progress.borrow().latest_message()
            {
                event_progress.set_message(message);
            }
            ProgressControl::Continue
        });

    if let Some(revisit_index) = options.revisit_index {
        session = session.revisit_index(revisit_index);
    }
    if let Some(limit) = options.limit {
        session = session.limit(limit);
    }

    let summary = session.run()?;
    progress.finish_and_clear();

    Ok(report_comment_run(
        &summary,
        &comment_progress.borrow(),
        options.output,
        quiet,
    ))
}

/// Report a finished session's failures and per-site progress.
fn report_comment_run(
    summary: &SessionSummary,
    comment_progress: &CommentRunProgress,
    output: &Path,
    quiet: bool,
) -> CommandOutcome {
    for failure in &summary.failures {
        log::warn!("Failed to capture {}: {}", failure.url, failure.error);
    }
    if let Some(error) = &summary.fatal_error {
        log::warn!("The session ended early: {error}");
    }
    if summary.cancelled {
        log::warn!("The session was interrupted");
    }

    for (site_url, snapshot) in comment_progress.iter() {
        if let Some(snapshot) = snapshot {
            if let Some(shortfall) = snapshot.visibility_shortfall() {
                log::warn!(
                    "WordPress counted {} comments for {} before visibility filtering but returned {} visible comments ({shortfall} omitted)",
                    snapshot.total,
                    site_url,
                    snapshot.downloaded
                );
            }
            if !quiet {
                println!("{site_url}: {snapshot} to {}", output.display());
            }
        } else if !quiet {
            println!(
                "Downloaded no comments from {site_url} to {}",
                output.display()
            );
        }
    }

    if summary.is_complete() {
        CommandOutcome::Success
    } else {
        log::warn!("a partial archive was published at {}", output.display());

        CommandOutcome::ReportedProblems
    }
}

/// Load the archiver settings, making the output filename authoritative for WARC compression.
fn load_config_for_output(config: Option<&Path>, output: &Path) -> Result<Config, Error> {
    let mut config: Config = config::load(config)?;
    config.gzip_warc = output
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"));

    Ok(config)
}

/// Drive each site's comment traversal in turn, reporting progress as its pages are inspected.
struct ProgressingCommentDriver {
    drivers: Vec<CommentDriver>,
    /// The index of the driver whose request is outstanding.
    active: Option<usize>,
    progress: Rc<RefCell<CommentRunProgress>>,
}

struct CommentRunProgress {
    site_urls: Vec<String>,
    snapshots: Vec<Option<CommentProgress>>,
    latest: Option<usize>,
}

impl CommentRunProgress {
    fn latest_message(&self) -> Option<String> {
        let index = self.latest?;
        Some(format!(
            "{}: {}",
            self.site_urls[index], self.snapshots[index]?
        ))
    }

    fn iter(&self) -> impl Iterator<Item = (&str, Option<CommentProgress>)> + '_ {
        self.site_urls
            .iter()
            .map(String::as_str)
            .zip(self.snapshots.iter().copied())
    }
}

impl Driver for ProgressingCommentDriver {
    fn next(&mut self) -> Option<Request> {
        let (index, request) = self
            .drivers
            .iter_mut()
            .enumerate()
            .find_map(|(index, driver)| driver.next().map(|request| (index, request)))?;
        self.active = Some(index);

        Some(request)
    }

    fn inspect(&mut self, capture: &Capture<'_>) -> Inspection {
        let Some(index) = self.active.take() else {
            return Inspection::error(format!(
                "captured an unrequested WordPress comments URL: {}",
                capture.url
            ));
        };
        let driver = &mut self.drivers[index];
        let inspection = driver.inspect(capture);
        let mut progress = self.progress.borrow_mut();
        progress.snapshots[index] = driver.progress();
        progress.latest = Some(index);
        inspection
    }

    fn failed(&mut self, url: &str, error: &archivindex_archiver::Error) {
        if let Some(index) = self.active.take() {
            self.drivers[index].failed(url, error);
        }
    }
}

/// Read, sort, and deduplicate `WordPress` comments captured in a WARC file.
///
/// Comments captured with conflicting contents are logged as warnings, and the exit status reflects
/// that some were found.
fn read_wp_comments(options: ReadCommentsOptions) -> Result<CommandOutcome, Error> {
    let result = read_comments(options.input)?;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();

    for comment in result.comments {
        serde_json::to_writer(&mut output, &comment)?;
        writeln!(output)?;
    }

    for warning in &result.warnings {
        log::warn!(
            "Conflicting objects for WordPress comment {}: {} != {}",
            warning.id,
            warning.first,
            warning.second
        );
    }

    Ok(CommandOutcome::from_reported_problems(
        !result.warnings.is_empty(),
    ))
}

/// Check that every page advertised in a comments WARC has a qualifying capture record.
fn check_wp_comments(options: &CheckCommentsOptions, quiet: bool) -> Result<CommandOutcome, Error> {
    let collections = check_comment_collections(&options.input)?;
    if collections.is_empty() {
        log::warn!(
            "{} has no qualifying WordPress comments capture",
            options.input.display()
        );
        if !quiet {
            println!("{} is incomplete", options.input.display());
        }
        return Ok(CommandOutcome::ReportedProblems);
    }

    let mut reported_problems = false;
    for collection in collections {
        let coverage = collection.coverage;
        let complete = coverage.is_complete();
        let total_changed = coverage.advertised_total_changed();
        reported_problems |= !complete || total_changed;

        if let Some(warning) = page_total_change_warning(&coverage) {
            log::warn!("{}: {warning}", collection.endpoint);
        }
        if complete {
            if !quiet {
                println!(
                    "{} is complete for {}: all {} advertised comment pages were captured",
                    options.input.display(),
                    collection.endpoint,
                    coverage
                        .total_pages
                        .expect("complete coverage has an advertised page count")
                );
            }
            continue;
        }

        match coverage.total_pages {
            None => log::warn!(
                "{} has no qualifying record with a valid X-WP-TotalPages header for {}",
                options.input.display(),
                collection.endpoint
            ),
            Some(total_pages) => {
                let missing_count = coverage
                    .missing_page_count()
                    .expect("an advertised page count has a missing-page count");
                let mut missing = coverage.missing_pages();
                let shown = missing.by_ref().take(20).collect::<Vec<_>>();
                let suffix = (missing_count > shown.len())
                    .then(|| format!(" (and {} more)", missing_count - shown.len()));
                log::warn!(
                    "{} is missing qualifying records for {} of {} advertised pages for {}: {}{}",
                    options.input.display(),
                    missing_count,
                    total_pages,
                    collection.endpoint,
                    shown
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    suffix.as_deref().unwrap_or("")
                );
            }
        }
        if !quiet {
            println!(
                "{} is incomplete for {}",
                options.input.display(),
                collection.endpoint
            );
        }
    }

    Ok(CommandOutcome::from_reported_problems(reported_problems))
}

/// Capture exactly the comment pages missing from an existing WARC.
fn complete_wp_comments(
    options: &CompleteCommentsOptions,
    quiet: bool,
) -> Result<CommandOutcome, Error> {
    let config: Config = config::load(options.config.as_deref())?;
    let request_delay = config.session.request_delay;
    let archiver = Archiver::new(config)?;
    let progress = spinner("Completing comments", None);
    let summary =
        complete_comments_with_delay(&archiver, &options.input, &options.output, request_delay)?;
    progress.finish_and_clear();

    report_completion_problems(&summary);
    if !quiet {
        if summary.missing_pages.is_empty() {
            println!(
                "{} was already complete; wrote its warcinfo record to {}",
                options.input.display(),
                options.output.display()
            );
        } else {
            println!(
                "Captured {} of {} missing comment pages to {}",
                summary.missing_pages.len() - summary.uncaptured_pages.len(),
                summary.missing_pages.len(),
                options.output.display()
            );
        }
    }

    Ok(CommandOutcome::from_reported_problems(
        !summary.is_complete(),
    ))
}

fn report_completion_problems(summary: &CommentCompletionSummary) {
    if let Some(archive) = &summary.archive {
        for failure in &archive.failures {
            log::warn!("Failed to capture {}: {}", failure.url, failure.error);
        }
        if archive.cancelled {
            log::warn!("comment completion was cancelled before every request was made");
        }
        let partial = archive.partial_captures();
        if partial > 0 {
            log::warn!("{partial} comment page captures were unexpectedly truncated");
        }
    }
    if !summary.uncaptured_pages.is_empty() {
        log::warn!(
            "no qualifying response was captured for comment pages {}",
            summary
                .uncaptured_pages
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

/// Describe the signed difference at every transition between advertised totals.
fn page_total_change_warning(coverage: &CommentCompleteness) -> Option<String> {
    if !coverage.advertised_total_changed() {
        return None;
    }

    let totals = coverage
        .advertised_page_totals
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" -> ");
    let differences = coverage
        .advertised_page_totals
        .windows(2)
        .map(|pair| {
            if pair[1] >= pair[0] {
                format!("+{}", pair[1] - pair[0])
            } else {
                format!("-{}", pair[0] - pair[1])
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    Some(format!(
        "X-WP-TotalPages changed over the WARC session ({totals}); successive differences: \
         {differences}"
    ))
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid WordPress base URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("invalid cookie: {0}")]
    Cookie(#[from] archivindex_archiver::CookieError),
    #[error("archiving error: {0}")]
    Archive(#[from] archivindex_archiver::Error),
    #[error("invalid archiver configuration: {0}")]
    Config(#[from] archivindex_archiver::ConfigError),
    #[error(transparent)]
    ConfigFile(#[from] config::Error),
    #[error(transparent)]
    UserAgent(#[from] archivindex_archiver::UserAgentError),
    #[error(transparent)]
    SessionId(#[from] archivindex_archiver::session::SessionIdError),
    #[error("WordPress comment reading error: {0}")]
    ReadComments(#[from] archivindex_wordpress_scraper::read::Error),
    #[error("WordPress comment completion error: {0}")]
    CompleteComments(#[from] archivindex_wordpress_scraper::complete::Error),
    #[error("WordPress archive resume inspection error: {0}")]
    ResumeInfo(#[from] archivindex_wordpress_scraper::resume::Error),
    #[error("WordPress archive lint error: {0}")]
    Lint(#[from] archivindex_wordpress_scraper::lint::Error),
    #[error(transparent)]
    Combine(#[from] combine::Error),
    #[error("cannot read archive session directory {}: {source}", path.display())]
    SessionDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "{} contains no direct .warc or .warc.gz files belonging to session {session_name:?}",
        output.display()
    )]
    NoSessionWarcs {
        output: PathBuf,
        session_name: String,
    },
    #[error("archive segment timestamps are exhausted for session {0:?}")]
    SessionTimestampExhausted(String),
    #[error("the first archive segment {} does not contain every endpoint probe", .0.display())]
    MissingSessionProbes(PathBuf),
    #[error(
        "archive segment {} belongs to site {actual:?}, not the session's site {expected:?}",
        path.display()
    )]
    SessionSiteMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error(
        "archive segment {} uses cutoff {actual}, not the session's cutoff {expected}",
        path.display()
    )]
    SessionCutoffMismatch {
        path: PathBuf,
        expected: DateTime<Utc>,
        actual: DateTime<Utc>,
    },
    #[error("cannot derive an archive session name from {}", .0.display())]
    SessionName(PathBuf),
    #[error("cannot read comment update input {}: {source}", path.display())]
    UpdateInputRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("comment update directory {} contains no direct .warc or .warc.gz files", .0.display())]
    NoUpdateWarcs(PathBuf),
    #[error("cannot derive a comment update from {}: {source}", path.display())]
    UpdateAnchor {
        path: PathBuf,
        #[source]
        source: Box<archivindex_wordpress_scraper::read::Error>,
    },
    #[error(
        "the session stopped before its initial requests were finished, so a new archive must \
         start over; a partial archive was published at {}",
        .0.display()
    )]
    InitialRequestsIncomplete(PathBuf),
    #[error(
        "the archive in {} stopped before its initial requests finished and cannot be resumed; \
         start a new archive instead",
        .0.display()
    )]
    InitialArchiveCannotResume(PathBuf),
    #[error(
        "cannot recover the original before cutoff from {}; no paginated request was recorded",
        .0.display()
    )]
    MissingResumeCutoff(PathBuf),
    #[error("the archive session's initial probes do not include endpoint {0:?}")]
    UnknownEndpoint(String),
    #[error("comment update overlap is out of range: {0:?}")]
    OverlapOutOfRange(Duration),
    #[error("comment update window starts at {after}, which is not before {before}")]
    InvalidUpdateWindow {
        after: chrono::DateTime<Utc>,
        before: chrono::DateTime<Utc>,
    },
    #[error("JSON writing error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Parser)]
#[command(name = "archivindex-wordpress-scraper", version, author)]
struct Opts {
    #[command(flatten)]
    verbosity: Verbosity,
    #[command(subcommand)]
    command: Command,
}

/// The workflow to run.
#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Archive every supported collection a site exposes through its `WordPress` REST API v2.
    #[command(name = "archive")]
    Archive(ArchiveRunOptions),
    /// Check that every advertised comments page has a qualifying response or revisit record.
    #[command(name = "check-comments")]
    Check(CheckCommentsOptions),
    /// Combine a site's archive and resume-run segments into one WARC.
    #[command(name = "combine")]
    Combine(CombineOptions),
    /// Capture pages missing from a comments WARC into a new WARC.
    #[command(name = "complete-comments")]
    Complete(CompleteCommentsOptions),
    /// Validate a collection archive's initial captures and pagination series.
    #[command(name = "lint")]
    Lint(LintOptions),
    /// Write archived comments as JSONL, sorted and deduplicated by numeric ID.
    ///
    /// Use a single site's archive; conflicting versions produce warnings and a nonzero
    /// exit status.
    #[command(name = "read-comments")]
    Read(ReadCommentsOptions),
    /// Continue an archive by reading all WARC segments sharing its session name.
    #[command(name = "resume-archive")]
    ResumeArchive(ResumeArchiveOptions),
    /// Print the command continuing an incomplete collection-archive WARC.
    #[command(name = "resume-info")]
    ResumeInfo(ResumeInfoOptions),
    /// Capture new comments in a window overlapping an existing comments WARC.
    #[command(name = "update-comments")]
    Update(UpdateCommentsOptions),
}

/// Options for archiving a site.
#[derive(Clone, Debug, clap::Args)]
struct ArchiveRunOptions {
    /// A TOML or JSON archiver configuration file, recognized by its extension; every key is
    /// optional and takes its default when absent.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    config: Option<PathBuf>,
    /// Site host with an optional installation path, such as `example.com/blog`.
    /// HTTPS is the default; an explicit `http://` or `https://` scheme is also accepted.
    #[arg(long, value_name = "BASE")]
    base: Site,
    /// Directory the session's plain WARC file, named after the session, is written to; it is
    /// created when missing (an existing file is not overwritten).
    #[arg(short, long, value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    output: PathBuf,
    /// Session name shared by timestamped WARC segments. Defaults to the scheme-free base, with
    /// characters other than ASCII letters, digits, `-`, `.`, `_`, and `~` replaced by hyphens.
    #[arg(long)]
    session_name: Option<String>,
    /// Persistent payload-revisit and conditional-request state database.
    #[arg(long)]
    revisit_index: Option<PathBuf>,
    /// Stop after this many captures, reporting the command that continues the archive.
    #[arg(long)]
    limit: Option<usize>,
    #[command(flatten)]
    per_page: PerPageOptions,
    /// Cookie header obtained from a browser, scoped to the site's host.
    ///
    /// The value is sent with every request to that host and recorded in the WARC request records.
    /// Quote values containing semicolons.
    #[arg(long)]
    cookie: Option<String>,
}

/// Options for continuing an archive from its checkpoint.
#[derive(Debug, clap::Args)]
struct ResumeArchiveOptions {
    /// A TOML or JSON archiver configuration file.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    config: Option<PathBuf>,
    /// Directory containing every plain or compressed WARC segment from the archive run.
    #[arg(short, long, value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    output: PathBuf,
    /// Exact session name, excluding the segment timestamp and `.warc` or `.warc.gz` suffix.
    #[arg(long)]
    session_name: String,
    /// Original archive cutoff, required if the initial segment has no paginated request.
    #[arg(long, value_name = "TIMESTAMP")]
    before: Option<DateTime<Utc>>,
    /// Persistent payload-revisit and conditional-request state database.
    #[arg(long)]
    revisit_index: Option<PathBuf>,
    /// Stop this continuation after this many captures.
    #[arg(long)]
    limit: Option<usize>,
    #[command(flatten)]
    per_page: PerPageOptions,
    /// Cookie header obtained from a browser, scoped to the recovered site's host.
    #[arg(long)]
    cookie: Option<String>,
}

/// Options for recovering continuation information from an archive WARC.
#[derive(Debug, clap::Args)]
struct ResumeInfoOptions {
    /// Path of the plain or gzip-compressed WARC file to inspect.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    input: PathBuf,
}

/// Options for reading comments from a WARC file.
#[derive(Debug, clap::Args)]
struct ReadCommentsOptions {
    /// Path of the plain or gzip-compressed WARC file to read.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    input: PathBuf,
}

/// Options for checking comments page coverage in a WARC file.
#[derive(Debug, clap::Args)]
struct CheckCommentsOptions {
    /// Path of the plain or gzip-compressed WARC file to check.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    input: PathBuf,
}

/// Options for linting a `WordPress` collection archive.
#[derive(Debug, clap::Args)]
struct LintOptions {
    /// Path of the plain or gzip-compressed WARC file to lint.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    input: PathBuf,
}

/// Options for capturing pages missing from a comments WARC.
#[derive(Debug, clap::Args)]
struct CompleteCommentsOptions {
    /// A TOML or JSON archiver configuration file, recognized by its extension; every key is
    /// optional and takes its default when absent.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    config: Option<PathBuf>,
    /// Path of the plain or gzip-compressed WARC file to inspect.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    input: PathBuf,
    /// Path of the completion WARC to write; a `.gz` suffix enables gzip compression (an existing
    /// file is not overwritten).
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    output: PathBuf,
}

/// Options for incrementally updating an archived comments collection.
#[derive(Debug, clap::Args)]
struct UpdateCommentsOptions {
    /// A TOML or JSON archiver configuration file, recognized by its extension; every key is
    /// optional and takes its default when absent.
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    config: Option<PathBuf>,
    /// Existing comments WARC, or a directory of `.warc` and `.warc.gz` files used to plan updates.
    /// The input files are left unchanged; subdirectories are not searched.
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::AnyPath)]
    input: PathBuf,
    /// Path of the WARC file to write; a `.gz` suffix enables gzip compression (an existing file is
    /// not overwritten).
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    output: PathBuf,
    /// URL-safe name identifying the update session and its WARC file.
    #[arg(long)]
    session_name: String,
    /// Begin this far before each site's latest comment time, or its archived cutoff if no
    /// comment has a valid time.
    #[arg(long, default_value = "1day", value_parser = parse_duration)]
    overlap: Duration,
    /// Persistent payload-revisit and conditional-request state database.
    #[arg(long)]
    revisit_index: Option<PathBuf>,
    /// Stop successfully after capturing this many comment batches.
    #[arg(long)]
    limit: Option<usize>,
    /// Always perform a second complete sweep, even when the first sweep's totals are consistent.
    #[arg(long)]
    second_sweep: bool,
    /// Cookie header obtained from a browser, scoped to every archived site's host.
    #[arg(long)]
    cookie: Option<String>,
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    humantime::parse_duration(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
