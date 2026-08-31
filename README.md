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

| Crate                                                     | Description                                                        |
| --------------------------------------------------------- | ------------------------------------------------------------------ |
| [`archivindex-wordpress-model`](crates/model/)            | Data models for the WordPress REST API                             |
| [`archivindex-wordpress-scraper`](crates/scraper/)        | Capturing and reading WordPress REST API resources as WARC records |
| [`archivindex-wordpress-scraper-cli`](tools/scraper-cli/) | Scraper command-line tool                                          |

The library crates live under [`crates`](crates/), and the command-line application lives under
[`tools`](tools/).

## Development

The workspace requires Rust 1.97 or later. Run its tests and build its documentation with:

```console
cargo test --locked --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps
```

## License

This project is licensed under the [GNU General Public License, version 3][gpl-3.0]; see
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
