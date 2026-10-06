# Windows measurements

Measured 6 October 2026 on Windows 11 Home 10.0.26200, AMD Ryzen 5 5600X, approximately 32 GB RAM, WebView2 154.0.4258.53. Release profile: size optimization, link-time optimization, one codegen unit and stripped executable. Rust 1.90.0; Node 24.19.0.

## Startup and reading

| Scenario | Time |
| --- | ---: |
| Final release, first usable token-entry screen | 654 ms |
| Cached library, 1,000 articles, launch 1 | 669 ms |
| Cached library, 1,000 articles, launch 2 | 657 ms |
| Cached library, 1,000 articles, launch 3 | 672 ms |
| Open an already-cached article | 87 ms |

These are actual release-process launches through the native SQLite/credential/IPC paths. Timing starts before process creation and ends when the screen is visible to Playwright attached through WebView2's local debugging port. Attachment and polling add overhead. The library renders 60 cards per page. The OS and filesystem caches were warm; these are not cold-boot measurements.

The fixture contained original fictional metadata and one cached article, with no remote images. A temporary invalid credential represented an already-connected account; a loopback proxy blocked remote requests. Measurements therefore exclude network import, authentication and uncached content downloads. The normal application was used without test flags or alternate IPC. Disconnect removed the fixture credential and all account content afterward.

The real Windows close message flushed 24px typography and a text anchor. Reopening restored the font size and approximately 1,578px of scroll, with the anchor at the same passage. Fonts loaded locally and the rendered background was `rgb(0, 0, 0)`.

## Memory

Seven processes were measured: the Rust executable and its WebView2 browser, renderer, GPU, crash handler and utility children. Values are sums across that tree.

| Scenario | Private committed memory | Sum of working sets |
| --- | ---: | ---: |
| Token-entry screen | 169 MiB | 351 MiB |
| Library, 1,000 articles, first two runs | 191–193 MiB | 368–382 MiB |
| Reading a cached article | 263 MiB | 397 MiB |

Summed working sets count shared pages more than once. Private committed memory includes allocations that need not currently be resident. These figures include the WebView2 runtime and GPU allocations; the native executable alone used about 6.2 MiB private memory in the restart verification, while the renderer used about 52 MiB and the GPU process about 130 MiB. Hardware, runtime updates and article images will change these figures.

## Release sizes

| Artifact | Size |
| --- | ---: |
| Windows x64 executable | 6,312,960 bytes (6.02 MiB) |
| NSIS installer | 2,668,199 bytes (2.54 MiB) |
| Frontend JavaScript | 119.12 KB; 42.54 KB gzip |
| Frontend CSS | 11.28 KB; 3.05 KB gzip |
| Two local Inter fonts | 100.08 KB combined |
| Bundled dependency notices | 724,926 bytes |

The installer includes the executable and licence notices, and downloads WebView2 if missing. The installed WebView2 runtime is separate from these download sizes. The executable is unsigned.

Installer SHA-256: `3EC28097755B0B7A1E6AAC0344E97C4C7F40EDAC766AB34CE9BC474C2FB2B945`.

## Scope of verification

Frontend unit tests, real-browser interaction tests and native tests cover query membership, archive/highlight failure behavior, sanitized rendering, local positions, account isolation, pagination response contracts, shared transport cooldown, SQLite reconciliation and real EPUB parsing. The browser acceptance run uses controlled native IPC fixtures; the native smoke run uses the real release and persisted fixture data.

No live Readwise account was used. Successful remote archive/highlight mutations and a large real-account import still require checking with a token entered locally. The public API also constrains sync speed and progress support as described in the README.
