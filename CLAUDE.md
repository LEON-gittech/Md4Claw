# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AppFlowy is an open-source AI workspace (Notion alternative) built with **Flutter** (frontend) and **Rust** (backend core). The entire codebase lives under `frontend/` with two main subsystems that communicate via protobuf-based FFI.

## Build System

All builds use **cargo-make** from the `frontend/` directory. Install it with `cargo install cargo-make`.

### Build the Rust backend (required before running Flutter)
```bash
cd frontend
# Linux x86_64
cargo make --profile development-linux-x86_64 appflowy-core-dev
# macOS ARM
cargo make --profile development-mac-arm64 appflowy-core-dev
# macOS x86_64
cargo make --profile development-mac-x86_64 appflowy-core-dev
# Windows
cargo make --profile development-windows-x86 appflowy-core-dev
```

### Run code generation (protobuf + freezed + localization)
```bash
cd frontend
cargo make code_generation
# Or dry run (no protobuf rebuild, just Dart codegen):
cargo make dry_code_generation
```

### Build & run the full Flutter app
```bash
cd frontend
cargo make --profile development-linux-x86_64 appflowy-dev
# Then: cd appflowy_flutter && flutter run
```

### Clean everything (Rust + generated files)
```bash
cd frontend
cargo make flutter_clean
```

## Testing

### Rust unit tests
```bash
cd frontend
cargo make rust_unit_test
# Or directly:
cd frontend/rust-lib && cargo test --no-default-features
```

### Flutter unit tests (builds Rust test backend first)
```bash
cd frontend
cargo make dart_unit_test   # builds test backend + runs tests
cargo make dart_unit_test_no_build  # skip Rust rebuild if already built
```

### Single Flutter test file/case
```bash
cd frontend/appflowy_flutter
flutter test -j, --concurrency=1 "path/to/test_file.dart" --name "test case name"
```

### Flutter integration tests
Integration tests are in `frontend/appflowy_flutter/integration_test/`. They are split into numbered desktop runners (`desktop_runner_1.dart` through `desktop_runner_9.dart`) and a mobile runner.

### Rust event integration tests
```bash
cd frontend/rust-lib/event-integration-test
cargo test --features "cloud_test"
```

## Architecture

### Two-Layer System: Flutter ↔ Rust via FFI

```
Flutter (Dart UI)  ←--protobuf events--→  dart-ffi  ←→  Rust Core (flowy-core)
```

- **`frontend/appflowy_flutter/`** — Flutter app with BLoC state management
- **`frontend/rust-lib/`** — Rust workspace with domain-specific crates
- **`frontend/rust-lib/dart-ffi/`** — FFI bridge: exposes Rust functions to Dart via `allo_isolate`
- **`frontend/appflowy_flutter/packages/appflowy_backend/`** — Dart-side FFI bindings (generated protobuf + event dispatch)

### Communication Pattern
Flutter sends events (protobuf-encoded) through `lib-dispatch` (a plugin-based event dispatcher). Each Rust domain crate registers an `AFPlugin` with event handlers in its `event_map.rs`. Responses flow back as protobuf messages.

### Rust Crate Organization (`frontend/rust-lib/`)

| Crate | Purpose |
|-------|---------|
| `flowy-core` | App initialization, wires all managers together |
| `flowy-user` | User auth, session management |
| `flowy-folder` | Workspace/folder/view hierarchy |
| `flowy-document` | Document editor backend (collaborative via Yrs CRDT) |
| `flowy-database2` | Database views (grid, board, calendar) |
| `flowy-ai` | AI chat and features |
| `flowy-search` | Full-text search (Tantivy) |
| `flowy-storage` | File/object storage |
| `flowy-notification` | Push notifications from Rust → Dart |
| `flowy-server` / `flowy-server-pub` | Cloud API client (AppFlowy Cloud) |
| `collab-integrate` | CRDT collaboration layer (wraps AppFlowy-Collab) |
| `lib-dispatch` | Event dispatcher framework |
| `lib-infra` | Shared utilities, task scheduling |
| `*-pub` crates | Public interfaces/traits for each domain |

### Flutter App Structure (`frontend/appflowy_flutter/lib/`)

| Directory | Purpose |
|-----------|---------|
| `startup/` | App bootstrap, dependency injection (GetIt), launch tasks |
| `plugins/document/` | Document editor (uses `appflowy_editor` package) |
| `plugins/database/` | Grid, Board (Kanban), Calendar views |
| `plugins/ai_chat/` | AI chat interface |
| `workspace/` | Sidebar, tabs, settings, workspace management |
| `user/` | Authentication, user profile |
| `mobile/` | Mobile-specific UI |
| `features/` | Newer feature modules (share, settings, workspace) |
| `shared/` | Cross-feature shared utilities |

### Internal Flutter Packages (`frontend/appflowy_flutter/packages/`)
- `appflowy_backend` — Generated Dart FFI bindings and protobuf models
- `appflowy_ui` — Design system components
- `appflowy_popover` — Custom popover widget
- `appflowy_result` — Result type utilities
- `flowy_infra` / `flowy_infra_ui` — Infrastructure and UI utilities
- `flowy_svg` — SVG handling

### Dependency Injection
Uses `GetIt` as service locator. `DependencyResolver` in `startup/deps_resolver.dart` registers all services. The global `getIt` instance is in `startup/startup.dart`.

### State Management
Flutter BLoC pattern throughout. Blocs live in `application/` directories alongside their features. Presentation code is in `presentation/` directories.

## Code Style & Conventions

### Commit Messages
Commitlint enforced: `type: subject` format. Types: `build`, `chore`, `ci`, `docs`, `feat`, `feature`, `fix`, `refactor`, `style`, `test`. Max header 100 chars.

### Rust
- `rustfmt.toml`: 2-space indentation, 100 char max width, edition 2024
- Protobuf for all Rust↔Dart data types
- Each domain crate has `event_map.rs` (registers handlers) and a `protobuf/` dir for message definitions

### Dart/Flutter
- `analysis_options.yaml` enforces: `require_trailing_commas`, `prefer_final_locals`, `prefer_final_fields`, `unawaited_futures`, `sort_constructors_first`
- Generated files (`*.g.dart`, `*.freezed.dart`) are excluded from analysis
- Freezed for immutable data classes, `json_serializable` for JSON
- Translations in `frontend/resources/translations/` (easy_localization, keys in `en-US.json`)

## Key External Dependencies
- **AppFlowy-Collab** (Yrs/CRDT) — real-time collaboration, patched via `[patch.crates-io]` in `rust-lib/Cargo.toml`
- **AppFlowy-Cloud** (`client-api`) — cloud sync backend
- **appflowy_editor** — rich text document editor (git dependency)
- Update collab rev: `frontend/scripts/tool/update_collab_rev.sh <new_rev>`
- Update client-api rev: `frontend/scripts/tool/update_client_api_rev.sh <new_rev>`
