# NanoReader

A small Windows desktop app for reading articles and EPUBs saved in Readwise Reader.

Pure-black surfaces, cover and list browsing, adjustable typography, local reading positions, highlighting and archive. Built with Rust, Tauri and Svelte. Readwise remains your library; NanoReader has no server, analytics or AI service.

## Setup

Run the Windows installer or standalone executable built below. WebView2 is required; the installer downloads it if missing.

Get your [Readwise access token](https://readwise.io/access_token) and enter it in the app. NanoReader stores it in Windows Credential Manager. Inbox, Later and Books work immediately. Inbox is Reader's Inbox location; Reader's configurable Home dashboard is not imported.

Optional views live in `views.json` in the app's local data folder. **Settings → Open views file** opens it in Notepad. Copy named queries from your Reader views, save, then choose **Reload views**. See [views.example.json](views.example.json) and [supported queries](docs/views.md).

Home groups your configured views into sections with six article previews each, capped at 60 cards across the dashboard. **View all** opens the complete view. Without custom views, Home offers setup and the built-in tabs remain available. Search and sorting apply to Home as well as individual views.

Sync imports metadata first and fetches article bodies or books when opened. Reader limits LIST requests to 20 per minute, so a large first import takes time while completed pages remain browsable. **Refresh entire library** also reconciles deleted items and items moved back to Feed.

## Reading and API limits

Reading controls adjust font, size, weight, line spacing, paragraph spacing, text brightness and width, with a live text preview below the controls. The cover-size slider sits beside the library layout controls; its track and value appear on hover or keyboard focus. Cover cards ease into their new layout as the slider or window size changes. Preferences persist between launches. Transitions respect the system's reduced-motion setting. On Windows 11, the native title bar is pure black; older Windows versions retain the system's dark title bar.

Article HTML is sanitized; scripts, embedded frames and publisher styles are removed. Ordinary links open your browser. Article and cover images contact their source hosts when displayed.

Desktop reading positions are local. Reader supplies a percentage for an explicitly approximate resume; its documented API has no progress-write endpoint or saved-view configuration endpoint. EPUB percentages approximate a chapter. Highlights are matched by text in Reader; repeated passages can resolve to a different occurrence there. Local highlights preserve the selected occurrence.

EPUBs require a downloadable source from Reader. Encrypted, damaged or oversized books show an error; open these in Reader separately. PDF and video items can appear in the library; use Reader separately when readable HTML is unavailable. Readable items have an **Open in Reader** button. Offline operation is not guaranteed.

The local data directory is `%LOCALAPPDATA%\io.quietreader.desktop`. The original identifier is retained so upgrades from Quiet Reader keep the saved token, views and local reading positions. Each Windows account has its own directory; local files are excluded from Git. SQLite holds metadata, settings, local positions and a bounded article cache; downloaded EPUBs have a separate bounded cache. Disconnect removes the token and cached account content while retaining typography settings. Treat the data folder as private.

This is an independent client, unaffiliated with Readwise. [Reader API documentation](https://readwise.io/reader_api).

## Development

Install Node.js 22.12+ (or 24 LTS), Rust via rustup, Visual Studio C++ build tools with the Windows SDK, and WebView2. See [Tauri's Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows). `rust-toolchain.toml` pins Rust; commit both dependency lockfiles.

```powershell
npm ci
npm run desktop
```

Build the standalone executable and an NSIS installer:

```powershell
npm run package
```

Outputs are under `src-tauri/target/release/` and `src-tauri/target/release/bundle/nsis/`. Builds are unsigned. Measurements and their limits are in [docs/performance.md](docs/performance.md).

## Code and checks

`src-tauri/src/api.rs` owns HTTP requests and rate limits; `store.rs` owns SQLite. Native commands own credentials and account isolation. `books.rs` reads EPUB chapters and resources. `src/lib/library.ts` owns view matching; `content.ts` sanitizes article HTML and tracks text positions. Svelte components render the interface.

```powershell
npm run check
npm test
npm run build
npm run test:e2e
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

Browser tests use original fixtures and simulate only the native IPC boundary; native tests exercise HTTP contracts, SQLite and real EPUB parsing. Live-account testing needs a token entered locally in the app. Keep tokens, article bodies and app data out of issues and pull requests.

Contributions should keep dependencies and configuration small, make failures visible and document API limitations. Licensed under [MIT](LICENSE); bundled dependencies retain their [own licences](THIRD-PARTY-NOTICES.txt). Keep both notice files beside the executable when distributing a portable copy. Maintainers can refresh notices with `python tools/update-notices.py` after installing dependencies; it uses locked Cargo metadata and fetches omitted upstream licence files at their exact revisions.
