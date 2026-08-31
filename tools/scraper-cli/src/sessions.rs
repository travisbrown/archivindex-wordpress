//! Session filename grammar and a snapshot of the output directory.

use std::path::{Path, PathBuf};

use chrono::Utc;

use super::Error;

/// A timestamp suffix is reserved for segment identity, even in a user-chosen name. Short numeric
/// endings (such as `campaign-2026`) remain part of the session.
#[derive(Debug, Eq, PartialEq)]
pub struct SegmentName {
    pub session: String,
    timestamp: Option<u64>,
    legacy: Option<(u64, u64)>,
}

impl SegmentName {
    pub fn parse(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        let stem = name
            .strip_suffix(".warc.gz")
            .or_else(|| name.strip_suffix(".warc"))?;
        // Historical continuations used ~TIMESTAMP and ~TIMESTAMP~SEQUENCE.
        let (stem, legacy) = stem
            .rsplit_once('~')
            .and_then(|(prefix, suffix)| {
                if let Some((base, time)) = prefix.rsplit_once('~')
                    && let Some(time) = timestamp(time)
                    && let Some(sequence) = number(suffix)
                {
                    return Some((base, (time, sequence)));
                }
                timestamp(suffix).map(|time| (prefix, (time, 0)))
            })
            .map_or((stem, None), |(stem, legacy)| (stem, Some(legacy)));
        let (session, timestamp) = stem
            .rsplit_once('-')
            .and_then(|(session, time)| timestamp(time).map(|time| (session, Some(time))))
            .unwrap_or((stem, None));
        (!session.is_empty()).then(|| Self {
            session: session.to_owned(),
            timestamp,
            legacy,
        })
    }

    fn order(&self) -> (Option<u64>, Option<(u64, u64)>) {
        (
            self.timestamp.or_else(|| self.legacy.map(|(time, _)| time)),
            self.legacy,
        )
    }

    fn latest_timestamp(&self) -> Option<u64> {
        self.timestamp
            .into_iter()
            .chain(self.legacy.map(|(time, _)| time))
            .max()
    }
}

fn number(value: &str) -> Option<u64> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

fn timestamp(value: &str) -> Option<u64> {
    (value.len() >= 9).then(|| number(value)).flatten()
}

struct Segment {
    name: SegmentName,
    path: PathBuf,
    is_file: bool,
}

/// Read once per operation. This is a snapshot, not a reservation: publication must still refuse to
/// overwrite an output created after discovery.
pub struct SessionInventory {
    output: PathBuf,
    segments: Vec<Segment>,
}

impl SessionInventory {
    pub fn read(output: &Path) -> Result<Self, Error> {
        let read_error = |source| Error::SessionDirectory {
            path: output.to_owned(),
            source,
        };
        let mut segments = Vec::new();
        for entry in std::fs::read_dir(output).map_err(read_error)? {
            let path = entry.map_err(read_error)?.path();
            if let Some(name) = SegmentName::parse(&path) {
                let is_file = path.metadata().map_err(read_error)?.is_file();
                segments.push(Segment {
                    name,
                    path,
                    is_file,
                });
            }
        }
        segments.sort_by(|left, right| {
            left.name
                .order()
                .cmp(&right.name.order())
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(Self {
            output: output.to_owned(),
            segments,
        })
    }

    pub fn warcs(&self, session: &str) -> Result<Vec<PathBuf>, Error> {
        let paths: Vec<_> = self
            .segments
            .iter()
            .filter(|segment| segment.is_file && segment.name.session == session)
            .map(|segment| segment.path.clone())
            .collect();
        if paths.is_empty() {
            return Err(Error::NoSessionWarcs {
                output: self.output.clone(),
                session_name: session.to_owned(),
            });
        }
        Ok(paths)
    }

    pub fn next_name(&self, session: &str) -> Result<String, Error> {
        let now = u64::try_from(Utc::now().timestamp())
            .unwrap_or_default()
            .max(100_000_000);
        let latest = self
            .segments
            .iter()
            .filter(|segment| segment.name.session == session)
            .filter_map(|segment| segment.name.latest_timestamp())
            .max();
        let timestamp = match latest {
            Some(latest) => now.max(
                latest
                    .checked_add(1)
                    .ok_or_else(|| Error::SessionTimestampExhausted(session.to_owned()))?,
            ),
            None => now,
        };
        Ok(format!("{session}-{timestamp}"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Barrier};

    use archivindex_archiver::session::{Crawl, Session};
    use archivindex_archiver::{Archiver, Config};

    use super::{Error, SegmentName, SessionInventory};

    #[test]
    fn names_preserve_session_identity_and_legacy_continuations() {
        for (name, session, timestamp, legacy) in [
            ("site.warc", "site", None, None),
            ("site-1788032113.warc.gz", "site", Some(1_788_032_113), None),
            (
                "site-1788032113~1788032999~12.warc",
                "site",
                Some(1_788_032_113),
                Some((1_788_032_999, 12)),
            ),
            (
                "site~1788032999.warc",
                "site",
                None,
                Some((1_788_032_999, 0)),
            ),
            ("site~nightly.warc", "site~nightly", None, None),
            ("site~123.warc", "site~123", None, None),
            ("campaign-2026.warc", "campaign-2026", None, None),
            (
                "campaign-1788032113-1788032999.warc",
                "campaign-1788032113",
                Some(1_788_032_999),
                None,
            ),
            ("site-123456789x.warc", "site-123456789x", None, None),
        ] {
            assert_eq!(
                SegmentName::parse(Path::new(name)),
                Some(SegmentName {
                    session: session.to_owned(),
                    timestamp,
                    legacy,
                }),
                "{name}"
            );
        }
        for name in [
            ".warc",
            ".warc.gz",
            "site.warc.partial",
            "site.WARC",
            "-1788032999.warc",
            "site.txt",
        ] {
            assert!(SegmentName::parse(Path::new(name)).is_none(), "{name}");
        }
    }

    #[test]
    fn inventory_uses_exact_membership_and_numeric_continuation_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let expected = [
            "site.warc",
            "site-999999999.warc.gz",
            "site-999999999~1000000000~2.warc",
            "site-999999999~1000000000~10.warc",
            "site-1000000001.warc",
        ];
        for name in expected.into_iter().rev().chain([
            "site-other-9999999999.warc",
            "site-extra.warc",
            "site-100.warc",
            "site-9999999998.warc.partial",
            "unrelated.warc.gz",
        ]) {
            std::fs::write(directory.path().join(name), [])?;
        }
        std::fs::create_dir(directory.path().join("site-9999999997.warc"))?;
        let inventory = SessionInventory::read(directory.path())?;
        assert_eq!(
            inventory.warcs("site")?,
            expected.map(|name| directory.path().join(name))
        );
        // A directory can occupy an output name but cannot be read as an archive.
        assert_eq!(inventory.next_name("site")?, "site-9999999998");
        assert_eq!(
            inventory.warcs("site-100")?,
            [directory.path().join("site-100.warc")]
        );
        assert!(matches!(
            inventory.warcs("missing"),
            Err(Error::NoSessionWarcs { .. })
        ));
        Ok(())
    }

    #[test]
    fn discovery_errors_are_not_an_empty_inventory() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        assert!(matches!(
            SessionInventory::read(&directory.path().join("absent")),
            Err(Error::SessionDirectory { .. })
        ));
        let file = directory.path().join("file");
        std::fs::write(&file, [])?;
        assert!(matches!(
            SessionInventory::read(&file),
            Err(Error::SessionDirectory { .. })
        ));
        std::fs::write(directory.path().join(format!("site-{}.warc", u64::MAX)), [])?;
        assert!(matches!(
            SessionInventory::read(directory.path())?.next_name("site"),
            Err(Error::SessionTimestampExhausted(_))
        ));
        Ok(())
    }

    #[test]
    fn simultaneous_inventories_do_not_allow_overwriting_a_published_segment()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("site-9999999999.warc"), b"earlier")?;
        let barrier = Arc::new(Barrier::new(2));
        let publishers = [0, 1].map(|_| {
            let output = directory.path().to_owned();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let name = SessionInventory::read(&output)
                    .unwrap()
                    .next_name("site")
                    .unwrap();
                assert_eq!(name, "site-10000000000");
                barrier.wait();
                Session::new(
                    Archiver::new(Config::default()).unwrap(),
                    &name,
                    Crawl::seeds(Vec::<String>::new()),
                    output.join(format!("{name}.warc")),
                )
                .unwrap()
                .run()
                .is_ok()
            })
        });
        let successes = publishers
            .into_iter()
            .map(|publisher| usize::from(publisher.join().unwrap()))
            .sum::<usize>();
        assert_eq!(successes, 1);
        assert_eq!(
            std::fs::read(directory.path().join("site-9999999999.warc"))?,
            b"earlier"
        );
        assert!(std::fs::metadata(directory.path().join("site-10000000000.warc"))?.len() > 0);
        Ok(())
    }
}
