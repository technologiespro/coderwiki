# Changelog

## 1.17.0
- DeepWiki-style table of contents: the model now returns only chapters → pages with specific titles and path prefixes (compact JSON), files are assigned deterministically (longest matching prefix → shared words → same-directory neighbours → "Other files"). Previously the model had to list every file path in one JSON answer, local models broke it on large repos and the directory fallback ("Module src", "Project root") was used
- Page count scales with the repository (3-5 pages for tiny repos up to 20-30 for large ones); prompt forbids directory-name titles and generic filler, asks to split large directories by the deepest prefixes
- Repository outline (directories with file counts and first-line hints) + README excerpt, shrunk to the context budget
- JSON repair (fences, trailing commas) and a second attempt with feedback; the fallback plan is no longer cached forever and is localized (no hardcoded Russian "Модуль …" in other languages)
- Existing wikis keep their structure (diff updates reuse the previous plan); new generations use the new planner
- Rust unit tests for planner/quality without Tauri: `tests/rust_unit` (`cargo test`)

## 1.16.0
- Documentation quality: large files are no longer cut off from the top — they are split by top-level declarations (functions/classes), every part is summarized by the LLM (map) and the notes are merged within the section budget (reduce); small files of the section are still passed in full. Map/reduce notes are cached and shared between languages
- Related-files context (RAG) before generating each section: declarations of files imported by / importing the section (relative imports, `use crate::…`, `mod`, Python/JS module names) plus, when an embedding model is set, the most semantically similar code chunks from other sections. Embeddings are now computed BEFORE section generation (still used by chat)
- Strict prompts: target language enforced for every heading, table and diagram label; identifiers, paths and commands kept verbatim; mandatory coverage of public API with parameters/return types, internal logic, error handling and edge cases; no invented facts, no filler. Applied to sections, getting-started, overview, improvement ideas and single-section regeneration
- Generation parameters: temperature 0.2, top_p 0.9, top_k 40, frequency penalty 0.1 by default (was temperature 0.3 only); editable in Settings → "Generation parameters" with a "Recommended" reset
- New language versions ("+" in the sidebar) are translated from the open version instead of being generated from scratch: same chapter structure, Mermaid node IDs, code and paths preserved; long pages are translated in parts split at `##` headings; titles and chapter names translated in one request
- Overview now receives section excerpts proportional to the context budget (was 500 characters per section)

## web-docs 0.1.0
- New standalone app `/web-docs`: static multilingual documentation portal (VitePress 1.6) built from CoderWiki markdown folders or zip archives (`owner__repo@language-docs`)
- Sync script: zip extraction, transliterated URL slugs, internal link rewriting with broken-link report, SEO frontmatter (title/description/keywords/lang), `generated-manifest.json`, robots.txt
- Site: repository showcase with filter and gradient cards, per-repo sidebars (grouped when > 12 chapters), language badges in articles, interface language switcher, local search, Mermaid, dark mode, RTL for Arabic, UI strings for all 14 CoderWiki languages
- SEO: canonical, OpenGraph, hreflang alternates, sitemap.xml; `BASE_PATH`/`SITE_URL` via `.env`; GitHub Pages workflow `.github/workflows/web-docs-pages.yml`

## 1.15.0
- Collapsible chapters in the wiki sidebar: each chapter has a chevron that folds/unfolds its pages, a page counter is shown while folded, and the state is remembered per repository across sessions (`localStorage.collapsedChapters`); a page opened from search or by clicking a chapter title automatically unfolds its chapter
- Clear access-denied hint: when cloning fails with 403 / "Repository not found" / authentication errors, the error banner explains that a fine-grained GitHub PAT with the "Contents: Read-only" permission is required (different wording when a token is already saved)
- Desktop: "Recent .xdp packages" row on the home page — the last 5 imported `.xdp` files (via dialog or drag-and-drop) can be re-opened with one click; missing files are dropped from the list automatically
- ZSTD dictionary training: the genetic algorithm is now time-budgeted (default 30 s, up to 20 generations, population 8) with fitness caching for the elite, sample de-duplication, a token-redundancy-seeded initial genome and early stop after 6 stale generations; the dictionary is still accepted only when it actually shrinks the archive
- Large archives (> 8 MB) are compressed with an extended window (up to 128 MB) and long-distance matching, so repeats across the whole repository are found; `xdp-util` gained `--budget-ms`

## 1.14.0
- Desktop: the "Add repository" card now has a dedicated "Open .xdp" button that opens a system file dialog filtered to `.xdp` packages and imports them directly from disk; clicking the card body still picks a source folder, so the two actions no longer conflict
- Desktop: drag-and-drop of folders and `.xdp` files now works — the app listens to Tauri's native drag-drop events (HTML5 drop never fires inside the desktop webview) and reads the dropped path natively; the card highlights while dragging over the window
- New Tauri commands: `pick_xdp_file`, `import_xdp_path`, `read_local_folder`
- Desktop app version (from Cargo.toml) is shown in the header next to the CoderWiki title

## 1.13.1
- Cancel button now reacts instantly: it immediately switches to a pulsing "Cancelling…" state, and the desktop backend aborts the in-flight LLM request within 200 ms instead of waiting for the current section to finish (plan, sections, embeddings, overview and ideas calls are all cancellable)
- Cancelling in the desktop app now correctly reports the "cancelled" status instead of an error
- The documentation-language dialog lost its bottom "Cancel" button and got an X close icon in the top-right corner instead

## 1.13.0
- Rebranded SmartWiki to CoderWiki: app header, empty-state title, page title, desktop window title and product name, package names and backend messages. The desktop data directory identifier is intentionally kept as `app.smartwiki.desktop` so existing local databases are preserved

## 1.12.0
- Tree-structured wiki sidebar (like DeepWiki): the section plan is now two-level — 3-7 chapters with 1-4 pages each; the sidebar renders chapter headers with indented child pages (clicking a chapter opens its first page), both in the finished wiki and in the live list during generation
- Each section stores its chapter in a new `group` field; old wikis without groups keep rendering as a flat list, and the flat plan format from the LLM is still accepted
- Chapters survive diff updates (the reused plan carries groups) and are preserved in the plan cache; the enforced "Installation & Usage" page inherits the overview chapter; web and desktop in sync

## 1.11.0
- Guaranteed "Installation & Usage" section (like deepwiki.com): right after the overview, every generated wiki now includes a getting-started page with prerequisites, step-by-step installation commands, configuration and usage examples taken from the README and manifest files (package.json, Cargo.toml, Makefile, Dockerfile, etc.)
- The section is enforced in code: if the LLM's plan lacks a getting-started section, it is inserted automatically with the README and root manifests; a specialized prompt reproduces real usage examples from the README accurately and forbids invented commands
- Works on fresh generation and on diff updates of existing wikis, in both web and desktop; the title is localized in 14 languages

## 1.10.0
- Private GitHub repositories: a "GitHub token" field in settings accepts a fine-grained or classic PAT (Contents: Read); the token is stored only on the device (localStorage), validated via the GitHub API with a "Check" button showing the account login, and injected into `git clone` through a one-shot `http.extraHeader` Basic authorization — it never appears in URLs, logs or `.git/config` (web and desktop)

## 1.9.0
- Diff updates: when regenerating an existing wiki, the previous section plan is reused — sections whose files did not change are taken from the previous version as-is (no LLM calls), only sections affected by modified/added/removed files are regenerated
- New files are assigned to existing sections by the LLM (with a path-prefix heuristic fallback), so section structure and links stay stable between updates
- The changes panel now also lists the sections that were actually regenerated (clickable), stored as `changes.sectionsUpdated`; implemented in both web (Node) and desktop (Rust)

## 1.8.1
- The progress bar now hides automatically a couple of seconds after generation completes instead of staying on screen
- The first wiki page now clearly identifies the project's purpose: the LLM is instructed to open with what the project is, what problem it solves and who it is for, and receives a README excerpt as extra context (web and desktop)

## 1.8.0
- New optional "Improvement Ideas" page: a toggle in settings (off by default, stored in localStorage) makes the LLM add an extra final section with project development ideas — architecture/code refactorings, performance and security suggestions, a new-features table with value/complexity, and quick wins — written in the documentation language
- Works in every generation type (GitHub, local folder, "+"-language) in both web and desktop; the page is localized (14 languages), cached, exported to .xdp/ZIP and shared like a regular section

## 1.7.1
- Fixed desktop error `invalid args 'repo' for command 'get_docs': missing required key repo` after generation finished: the Tauri `start_generate` command does not return a repoKey, so opening the finished wiki used an undefined key; the app now takes the repoKey from the progress event (works for regular, local-folder and "+"-language generation in both web and desktop)

## 1.7.0
- Multi-language .xdp export: the .xdp button now packs ALL language versions of the repository into a single container (format xdp/2: `records.json` with every version + shared sources and RAG index without duplication); the exported file is named after the repository base without the language suffix
- Import of a multi-language .xdp brings in every version at once and shows a dialog to pick which language to open; single-language and legacy xdp/1 containers keep importing as before
- Backward compatible both ways: `record.json` of the exported version is still included for older importers

## 1.6.0
- One-click generation of a missing language version: a "+" button in the wiki sidebar language switcher opens the list of not-yet-generated languages; picking one starts generation from the cached sources (no re-cloning) while the current wiki stays readable, a live spinner badge appears next to the switcher, and the wiki switches to the new version when done
- New endpoint `POST /api/generate-lang` (web) / `start_generate_lang` command (desktop) that generates another language version from the sources stored in the database — works for GitHub and local repositories alike

## 1.5.0
- Language switcher inside an open wiki: badges with all generated language versions of the repository appear in the sidebar under the repo name; one click switches the whole wiki to that version
- Wiki page title shows the clean repository name (without the @language suffix)

## 1.4.0
- Multi-language documentation per repository: each language version is stored separately under `owner/name@language` (DB records and docs folders like `owner__name@chinese`), nothing gets overwritten
- Source code and the RAG index are shared between language versions of the same repository (no duplication); they are removed only when the last language version is deleted
- Home page shows a separate card per language version; the Docs Language filter now truly separates versions; .xdp export targets a specific language version
- Backward compatible: old records without `@language` keep working

## 1.3.0
- Finished sections can be opened right during generation: completed items in the live sidebar list become clickable and show the rendered page (with a "Live progress" button to return to the streaming view); when generation completes, the opened section stays selected
- New endpoint/command `live-section` (web) / `get_live_section` (desktop) that serves the already-saved section .md from disk

## 1.2.0
- Home page: language badge (EN, RU, 中文, …) on every wiki card for quick visual distinction
- Incremental saving: each section .md file is written to disk immediately after it is generated (web and desktop); on cancel, finished pages are preserved and the section cache skips regenerating unchanged sections on the next run
- Live section list in the left sidebar during generation: planned sections appear at once with pending / spinning / done indicators

## 1.1.1
- Fix (desktop): `start_generate` failed with `invalid type: string "100", expected usize` — numeric settings (max files, context tokens) are now accepted both as numbers and strings in all Tauri commands (generate, chat, section regeneration)
- Home page: the Docs Language filter is now placed to the right of the search bar instead of below it

## 1.1.0
- UI internationalization (i18n): English (default) and Russian UI languages, dropdown in the top bar before the settings icon
- Documentation language selection moved out of the top bar: clicking Generate now opens a modal with 14 documentation languages (English, Russian, Chinese, Japanese, Spanish, German, Portuguese, French, Arabic, Hindi, Indonesian, Vietnamese, Kazakh, Swahili); generation starts after a language is picked
- Home page: "Docs Language" filter dropdown after the search bar (default: all languages)
- All main UI strings translated (header, settings, home, progress, sidebar, viewer, chat)

## 1.0.0
- DeepWiki-style wiki generation: LLM plans thematic sections, then writes a page per section with mermaid diagrams
- Local RAG chat with agent tools (READ_DIR / READ_FILE / READ_FILE_CHUNK), architecture chat mode
- .xdp container format (XDP2): ZSTD level 19 with a genetically trained dictionary; shared Rust core (xdp-rust lib) used by the CLI utility and the desktop app
- Tauri desktop app (frontend/src-tauri): full server logic ported to Rust modules, SLED database, no Node.js server; system data folder (sled / repos / docs); MSI bundle target
- LM Studio integration: model list, streaming generation, embeddings API; connection test button and startup status indicator in the header
- Diagram lightbox with zoom, mouse panning and wheel zoom; mermaid sanitizer fixes quoted node IDs and renders untagged graph blocks
- Export: ZIP of markdown sections, .xdp export/import with dictionary training progress; share links (web version)
