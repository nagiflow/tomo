# Contributing to Tomo

Bug reports, fixes, docs and new tool integrations are welcome. For anything bigger than a small fix, **open an issue first** so we can agree on the approach.

## Build from source

### Prerequisites

- macOS with Xcode Command Line Tools (`xcode-select --install`)
- [Node.js](https://nodejs.org) 22 or newer
- [pnpm](https://pnpm.io) 12
- Stable [Rust](https://rustup.rs)

### Run the desktop app

```sh
git clone https://github.com/nagiflow/tomo.git
cd tomo
pnpm install
pnpm desktop:dev
```

The app starts with empty local data and walks you through onboarding. Debug builds keep the encryption key in a user-only file in the app data folder instead of the Keychain, so macOS doesn't ask for Keychain approval after every rebuild.

### Try it without any accounts

```sh
pnpm dev
```

Open <http://127.0.0.1:1420>. The browser preview uses sample conversations and keeps its state in the tab's session storage. It **does not connect accounts or send email**, which makes it the quickest way to work on the UI.

### Build an app bundle

```sh
pnpm desktop:build                          # release bundle
pnpm desktop:build --debug --bundles app    # faster development bundle
```

Bundles are written to `src-tauri/target/release/bundle/`. Local builds get an ad hoc signature; set `APPLE_SIGNING_IDENTITY` to sign with your own certificate. A build without a stable certificate asks for Keychain approval once per launch; approve it to keep access to your encrypted data.

## Architecture

```
React + TypeScript UI (Zustand, Radix)        src/
        │  typed Tauri commands
Rust backend                                  src-tauri/src/
  ├─ engine.rs      polling, triage, planning pipeline
  ├─ plans.rs       plan generation, validation, execution
  ├─ ai.rs          OpenAI Responses API with strict JSON schemas
  ├─ connectors.rs  Gmail / Outlook · slack.rs · workspace.rs (Google tools) · codex.rs
  ├─ policy.rs      deterministic auto-send safety checks
  ├─ auth.rs        OAuth + PKCE through the system browser
  └─ storage.rs     encrypted SQLite
```

Tomo calls official provider APIs only and never automates a browser to read messages.

## Making a change

1. Fork the repo, create a branch, and make your change.
2. Run the same checks CI runs:

   ```sh
   pnpm build && pnpm test
   pnpm exec playwright install chromium && pnpm test:e2e
   cargo fmt --manifest-path src-tauri/Cargo.toml --check
   cargo test --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   ```

3. Open a pull request that says what changed and how you tested it, with a screenshot for UI changes. `main` only accepts squash-merged pull requests whose checks pass.

## Guidelines

- Keep it small: the shortest change that solves the problem, matching the style of the code around it.
- Anything that sends, posts, shares or deletes must stay behind human approval and the checks in `policy.rs`. A change that weakens a safety check needs a clear reason in the PR.
- Never commit credentials, OAuth client secrets or real message content. Use the sample data in `src/services/preview.ts`.
- Changed the UI? Refresh the README images: with `pnpm dev` running, run `node scripts/readme-media.mjs` (needs `ffmpeg`).

The fixture tests can't prove live sign-in, real AI inference or delivery. [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) records what has been verified.

Maintainers: releases are covered in [docs/RELEASING.md](docs/RELEASING.md).
