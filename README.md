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

## Capture backends

The built-in recorder is the default. To enable the experimental wreq backend, build the CLI
with its `wreq` feature:

```console
cargo build --release -p archivindex-wordpress-scraper-cli --features wreq
```

This feature requires Rust 1.98 or later, a C and C++ compiler, CMake, and libclang. It builds
BoringSSL from source and uses the observer-enabled wreq fork pinned in `Cargo.lock`. The workspace
also patches `btls`, `btls-sys`, and `tokio-btls` to the source revision required by that fork.
The shared HTTP crates come from `archivindex`, and the archiver and WARC crates come from
`archivindex-warc`. Both use `main`, with exact commits pinned in `Cargo.lock`. No local checkout
of either repository is required.

The `archive`, `resume-archive`, `complete-comments`, and `update-comments` commands then accept
`--backend recorder|wreq` and `--profile NAME`, matching the archiver CLI. The profile defaults
to `chrome_136` and applies only to wreq:

```console
archivindex-wordpress-scraper archive --base example.com --output archives \
  --backend wreq --profile chrome_136
```

Both backends use the configured proxy, timeouts, response limits, cookies, and session settings.
When an archive run stops early, its printed resume command preserves the selected backend and
profile. The wreq backend negotiates HTTP/2 over HTTPS with HTTP/1 fallback. HTTP/2 messages are
reconstructed for WARC storage and marked with `WARC-Protocol: h2`; HTTP/1 capture preserves wire
bytes. It always uses the profile's `User-Agent`, ignoring the configuration file's `user-agent`
setting. New `warcinfo` records use that profile value too. Other configured headers take precedence
over profile headers. The `warcinfo` software field also identifies the wreq version and profile.

To test a profile with wreq's normal protocol negotiation, including HTTP/2, use:

```console
archivindex-wordpress-scraper wreq-test --base example.com --profile firefox_135
```

This command requires the `wreq` feature and requests `https://example.com/wp-json` directly
with wreq, following up to ten redirects. It writes the decoded response body to standard output,
or reports an error and exits with a nonzero status if the request fails. Add `-v` to log the
response status and negotiated HTTP version to standard error. Add `--http-1` to force HTTP/1.1
while keeping the profile and other request behavior the same. This command does not use archive
configuration or write WARC output.

## SOCKS proxies

The `archive`, `resume-archive`, `complete-comments`, and `update-comments` commands accept
`--proxy URI`, with the same behavior as the archiver CLI:

```console
archivindex-wordpress-scraper archive --base example.com --output archives \
  --proxy socks5h://127.0.0.1:1080
```

Alternatively, set the top-level `proxy` key in a TOML or JSON configuration file. `--proxy`
takes precedence. Use `socks5h://` to resolve destination hostnames through the proxy, or
`socks5://` for local DNS. Both support username/password authentication, for example
`socks5h://user:password@host:1080`; percent-encode reserved characters in credentials.
No proxy is used by default, and environment proxy settings are ignored.

All requests, including redirects and retries, use the configured proxy. Proxy failures never
fall back to direct connections. Printed resume commands omit `--proxy` because its URI may
contain credentials, so supply it again when resuming or set it in the configuration file.

## Development

The workspace requires Rust 1.98 or later; the optional `wreq` feature also requires the native
build tools listed above. Install the workspace policy checker with:

```console
cargo install --locked --git https://github.com/travisbrown/archivindex \
  --rev e788abfd91c924d915175b32f3c453eba9ede4be cargo-archivindex-build
```

Check the workspace policy, run its tests, and build its documentation with:

```console
cargo archivindex-build check
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
