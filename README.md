# archivindex-wordpress

![GitHub last commit][last-commit-badge]
[![build][build-badge]][build]
[![codecov][codecov-badge]][codecov]
[![license][license-badge]][gpl-3.0]
[![crates.io][crates-version-badge]][crates]
[![crates.io][crates-downloads-badge]][crates]
[![API Docs][docs-badge]][docs]

Rust libraries for reading WordPress REST API resources and capturing them in web archives.

## Crates

| Crate                                                     | Description                                                 |
| --------------------------------------------------------- | ----------------------------------------------------------- |
| [`archivindex-wordpress-model`](crates/model/)            | Data models for the WordPress REST API                      |
| [`archivindex-wordpress-scraper`](crates/scraper/)        | Capture and read WordPress REST API resources in WARC files |
| [`archivindex-wordpress-scraper-cli`](tools/scraper-cli/) | Scraper command-line tool                                   |

The library crates live under [`crates`](crates/), and the command-line application lives under
[`tools`](tools/).

## Archive sessions

The `archive` command writes each run to `SESSION-TIMESTAMP.warc`. The `resume-archive`
command reads segments with that exact session name in numeric timestamp and continuation order.
It also accepts `.warc.gz` files and legacy `~TIMESTAMP` or `~TIMESTAMP~SEQUENCE` suffixes.
A timestamp suffix has at least nine decimal digits, so `campaign-2026` remains a session name.
For example, `site-extra-1788032113.warc` belongs to the `site-extra` session, not to `site`.

Resuming requires the initial segment's complete set of endpoint probes. If the initial segment
contains no paginated request, supply its original cutoff with `--before`. New segments are
published without overwriting an existing file: a run whose output name is taken by another
process after discovery fails rather than replacing it.

The `combine` command selects files by a `--domain` prefix followed by a hyphen and reads them in
filename order. This can include more than one session; for example, `--domain site` also selects
`site-extra-1788032113.warc`.

## Development

The workspace requires Rust 1.97 or later. Run its tests and build its documentation with:

```console
cargo test --locked --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps
```

## License

This project is licensed under the [GNU General Public License, version 3 only][gpl-3.0]; see
[LICENSE][license] for the full text.

[build]: https://github.com/travisbrown/archivindex-wordpress/actions/workflows/ci.yml
[build-badge]: https://github.com/travisbrown/archivindex-wordpress/actions/workflows/ci.yml/badge.svg
[codecov]: https://codecov.io/gh/travisbrown/archivindex-wordpress
[codecov-badge]: https://codecov.io/gh/travisbrown/archivindex-wordpress/branch/main/graph/badge.svg
[crates]: https://crates.io/crates/archivindex-wordpress-scraper/
[crates-downloads-badge]: https://img.shields.io/crates/d/archivindex-wordpress-scraper
[crates-version-badge]: https://img.shields.io/crates/v/archivindex-wordpress-scraper.svg
[docs]: https://docs.rs/archivindex-wordpress-scraper/
[docs-badge]: https://docs.rs/archivindex-wordpress-scraper/badge.svg
[gpl-3.0]: https://www.gnu.org/licenses/gpl-3.0.html
[last-commit-badge]: https://img.shields.io/github/last-commit/travisbrown/archivindex-wordpress
[license]: LICENSE
[license-badge]: https://img.shields.io/badge/license-GPL--3.0-orange
