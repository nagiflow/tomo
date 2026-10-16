<div align="center">

<img src="src-tauri/icons/128x128@2x.png" width="112" alt="Tomo app icon">

# Tomo

**A local-first assistant for your inbox that does the work and asks before it acts.**

Tomo reads new Gmail, Outlook and Slack messages, sorts them, drafts replies in your own writing style, and prepares a plan for each one (move the meeting, share the file, then reply). Nothing runs until you approve it.

[![Verify](https://github.com/nagiflow/tomo/actions/workflows/ci.yml/badge.svg)](https://github.com/nagiflow/tomo/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platform: macOS 12+](https://img.shields.io/badge/platform-macOS%2012%2B-lightgrey)
![Built with Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202-24c8db)

[Features](#features) · [Download](#install) · [Connect your accounts](#connect-your-accounts) · [Privacy](#local-data-and-privacy) · [Contributing](#contributing)

<br>

<img src="docs/media/demo.gif" width="860" alt="Tomo demo: open a message, edit the reply in Tomo's plan, approve it, and find it in Activity">

<sub>Recorded in the browser preview with sample conversations. No real accounts or sends.</sub>

</div>

## Features

- **Plans, not just drafts.** For each message Tomo proposes ordered steps across your tools, such as moving a Google Calendar event, sharing a Drive file, creating a Doc, Sheet or Slides deck, posting to Slack, or handing a task to Codex. A reply comes last and is sent only after the earlier steps succeed.
- **You stay in control.** Edit any step, remove the ones you don't want, or tell Tomo in plain words what to change ("make it 5:30 and add a Meet link"). Writes always wait for your approval unless you opt in to more.
- **Writes like you.** Tomo learns a `style.md` from your sent mail. You can read and edit it.
- **Standing rules.** `rules.md` holds instructions such as "when an invoice arrives, post it in #finance".
- **Lives in the menu bar.** Review and approve from a compact popover. Closing the main window leaves Tomo running in the background.
- **Explains itself.** Every decision lands in a searchable Activity log with the reason, the risk and exactly what was sent.
- **Local-first and encrypted.** Messages, drafts and history are AES-256-GCM encrypted in a local SQLite database. There is no Tomo server.

<table>
  <tr>
    <td width="66%"><img src="docs/media/inbox.png" alt="Inbox showing a message, its risk level and Tomo's two-step plan"></td>
    <td width="34%"><img src="docs/media/popover.png" alt="Menu bar popover with Tomo's plan for a message"></td>
  </tr>
  <tr>
    <td align="center"><sub>Inbox: the message, why Tomo asked, and its plan</sub></td>
    <td align="center"><sub>Menu bar popover</sub></td>
  </tr>
  <tr>
    <td><img src="docs/media/activity.png" alt="Activity log of decisions Tomo made"></td>
    <td><img src="docs/media/settings.png" alt="Settings with AI provider and writing style"></td>
  </tr>
  <tr>
    <td align="center"><sub>Activity: what happened and why</sub></td>
    <td align="center"><sub>Settings and <code>style.md</code></sub></td>
  </tr>
</table>

## Install

[**Download the latest Tomo.dmg**](https://github.com/nagiflow/tomo/releases/latest) (macOS 12 or newer, Apple silicon and Intel).

Open the `.dmg`, drag **Tomo** into Applications, and launch it. The app is signed and notarized by Apple, so it opens without security warnings and needs nothing else installed. Tomo then walks you through connecting your accounts.

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

The app starts with empty local data and walks you through onboarding.

### Try it without any accounts

```sh
pnpm dev
```

Open <http://127.0.0.1:1420>. The browser preview uses sample conversations and keeps its state in the tab's session storage. It **does not connect accounts or send email**.

### Build an app bundle

```sh
pnpm desktop:build                          # release bundle
pnpm desktop:build --debug --bundles app    # faster development bundle
```

Bundles are written to `src-tauri/target/release/bundle/`. Local builds get an ad hoc signature; set `APPLE_SIGNING_IDENTITY` to sign with your own certificate. A build without a stable certificate asks for Keychain approval once per launch; approve it to keep access to your encrypted data.

### Publishing a release

Pushing a tag that matches `version` in `src-tauri/tauri.conf.json` (for example `git tag v0.1.0 && git push origin v0.1.0`) runs [`release.yml`](.github/workflows/release.yml). It builds a universal `.dmg`, signs it with the Developer ID certificate, notarizes it with Apple, and attaches it to a draft GitHub release for a maintainer to publish. The workflow reads these repository secrets:

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | The "Developer ID Application" certificate exported as `.p12`, base64-encoded (`base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERTIFICATE_PASSWORD` | The password chosen when exporting the `.p12` |
| `KEYCHAIN_PASSWORD` | Any random string for the temporary CI keychain |
| `APPLE_API_KEY` | The Key ID of an App Store Connect API key (Users and Access → Integrations), used to notarize |
| `APPLE_API_ISSUER` | The Issuer ID shown above the keys on that page |
| `APPLE_API_KEY_P8` | The contents of that key's `AuthKey_<KeyID>.p8` file |

## Connect your accounts

Tomo has no shared OAuth app and no vendor server: you bring your own registrations, and your data goes straight from your Mac to the providers.

| Service | How it connects |
| --- | --- |
| **Gmail**, Calendar, Drive, Docs, Sheets, Slides, Contacts | Your own Google OAuth client |
| **Outlook** | Your own Microsoft app registration |
| **Slack** | Your own Slack app |
| **AI** | Continue with ChatGPT, or an OpenAI API key |
| **Codex** (optional) | Runs locally inside a folder you choose |

Step-by-step registration for each one is in **[docs/ACCOUNT_SETUP.md](docs/ACCOUNT_SETUP.md)**. Enter the client IDs during onboarding or under **Settings → Inboxes**. Sign-in always opens your system browser, and Tomo never asks for a mailbox password. Each inbox type can connect several accounts or workspaces.

After connecting AI, refresh the model list, pick a model, and turn on background processing consent. ChatGPT eligibility and usage allowances are set by OpenAI.

## How automation works

Out of the box, **every write waits for you**. You can loosen this step by step:

| Setting | What Tomo does on its own |
| --- | --- |
| Default | Sorts mail and drafts plans; nothing is sent |
| **Send the easy ones** | Sends its own reply to acknowledgments and routine requests, but only to people you have emailed before (any Slack DM or mention counts) |
| **Rules** that allow it | Posts to Slack right away; replies and other changes still wait |
| **YOLO mode** | Runs every plan as soon as it is made. A red flag shows in the status bar while this is on |

Model judgment is backed by deterministic checks. Sensitive topics, commitments, money and uncertainty always need your approval. "Never automate" for a sender or thread, pausing, and any failed step stop automation in every mode. A request a provider has already accepted cannot be recalled.

## Local data and privacy

- **Encrypted at rest.** Message content, drafts, account metadata, preferences, rules and history are AES-256-GCM encrypted before they reach SQLite. The key lives in the macOS Keychain and is read once per launch. OAuth tokens and API keys sit in a separate file encrypted with that key. Debug builds keep the key in a user-only file instead, so the Keychain doesn't prompt after every rebuild.
- **What leaves your Mac.** With your consent, a bounded amount of each new message and its thread is sent to the AI provider you chose, with `store: false`. That provider's retention terms still apply.
- **Disconnecting** an inbox deletes its credentials and its locally cached messages, drafts and history. It does not touch mail held by the provider; revoke access in your provider account if you want that too.
- **No double sends.** Send attempts are recorded before they go out. An uncertain outcome stays blocked until it is confirmed with the provider, and minimal send records survive history deletion and resets.

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

Tomo calls official provider APIs only and never automates a browser to read your messages.

## Contributing

Contributions are welcome: bug reports, fixes, docs, and new tool integrations.

1. **Open an issue first** for anything bigger than a small fix, so we can agree on the approach.
2. Fork the repo, create a branch, and make your change. Use `pnpm dev` for UI work; it needs no accounts.
3. Run the same checks CI runs:

   ```sh
   pnpm build && pnpm test
   pnpm exec playwright install chromium && pnpm test:e2e
   cargo fmt --manifest-path src-tauri/Cargo.toml --check
   cargo test --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   ```

4. Open a pull request that describes what changed and how you tested it. Include a screenshot for UI changes.

**Guidelines**

- Keep it small: the shortest change that solves the problem, matching the style of the code around it.
- Anything that sends, posts, shares or deletes must stay behind human approval and the checks in `policy.rs`. Changes that weaken a safety check need a clear reason in the PR.
- Never commit credentials, OAuth client secrets or real message content. Use the sample data in `src/services/preview.ts`.
- Changed the UI? Refresh the README images with `pnpm dev` running and then `node scripts/readme-media.mjs` (needs `ffmpeg`).

The fixture tests can't prove live sign-in, real AI inference or delivery. See [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) for what has been verified.

### Reporting security issues

Please don't open a public issue for a vulnerability. Report it privately through [GitHub security advisories](https://github.com/nagiflow/tomo/security/advisories/new).

## Roadmap

Microsoft Teams, local model inference, managed enterprise policies, and Windows distribution are planned for later phases.

## License

[MIT](LICENSE) © Tomo contributors
