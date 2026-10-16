# User-owned account setup

Tomo's Community Edition accepts registrations created by each user. Client IDs identify the application; inbox passwords never enter Tomo. Refresh tokens, API keys, and the Google Desktop client secret are kept in an encrypted file on this Mac; the key that unlocks it is the only thing Tomo stores in Keychain. Client IDs themselves are not secrets.

## Gmail

Tomo signs in to Google directly from the desktop app. It does not use the `gws` or `gcloud` command-line tools.

Tomo ships with no Google registration of its own: each user signs in through an OAuth client in their own Google Cloud project. Setup links to each of these Console pages.

1. Create or choose a project in [Google Cloud Console](https://console.cloud.google.com/).
2. Enable the Gmail API.
3. Configure the OAuth consent screen / Google Auth Platform branding, with audience **External**.
4. On the **Audience** page choose **Publish app** and confirm. Do not leave the status at Testing: Google then expires the sign-in every 7 days. Verification is not required for personal use; you will see a "Google hasn't verified this app" warning at sign-in, where you choose Advanced and continue.
5. Create an OAuth client with application type **Desktop app**.
6. Download the client's JSON and pick it with **Choose File…** in setup (or under Settings › Inboxes › Connect…). You can also paste the client ID and secret into **Use your own OAuth app registrations**.
7. Authenticate and grant the requested permissions in the system browser. Also enable the Calendar, Drive, Docs, Sheets, Slides and People APIs in the project for the tools you turn on.

Tomo requests `gmail.readonly` for messages and thread context, and `gmail.send` for approved replies. It does not request mailbox deletion access. Ignored messages are recorded locally; they are not moved or deleted in Gmail. Desktop OAuth uses PKCE and an HTTP loopback callback bound to `127.0.0.1` on an available port.

Testing-mode authorization and restricted-scope verification are governed by Google. Personal test registrations and widely distributed shared registrations have different requirements; open-source licensing does not waive Google's verification rules. See [Google's desktop OAuth guide](https://developers.google.com/identity/protocols/oauth2/native-app) and [Gmail scopes](https://developers.google.com/workspace/gmail/api/auth/scopes).

## Outlook

1. Open [Microsoft Entra app registrations](https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade) and create a registration.
2. If you want both personal and work accounts, choose **Accounts in any organizational directory and personal Microsoft accounts**.
3. Add a **Mobile and desktop applications** platform. Configure the loopback redirect described below.
4. Add delegated Graph permissions: `User.Read`, `Mail.ReadWrite`, and `Mail.Send`. Tomo also requests identity scopes and `offline_access` for renewable sessions.
5. Copy the **Application (client) ID** into Tomo. Do not create or enter a Microsoft client secret; this is a public desktop client.
6. Choose **Connect…** for Outlook and authenticate in the system browser. Work accounts may require administrator consent.

Register `http://localhost/auth/callback` as the **Mobile and desktop applications** redirect URI. For Outlook only, Tomo uses `http://localhost:<available-port>/auth/callback` with a listener bound to `127.0.0.1`; Microsoft's localhost matching ignores the ephemeral port while preserving the path. Gmail and ChatGPT use an IP-literal loopback URI. See [Microsoft redirect URI constraints](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url).

`Mail.ReadWrite` is needed to create a reply draft with a stable immutable ID before sending; `Mail.Send` is a separate delegated permission. Recording that draft identity allows Tomo to reconcile uncertain send outcomes without blindly retrying. See [createReply permissions](https://learn.microsoft.com/en-us/graph/api/message-createreply?view=graph-rest-1.0) and [immutable Outlook IDs](https://learn.microsoft.com/en-us/graph/outlook-immutable-id).

Tenant policy can deny authorization even with a correctly registered client. Tomo respects this and allows other inboxes to continue. It does not use application-wide mailbox permissions.

## Slack

Tomo connects to Slack through a small app you create in your own workspace, so no workspace admin is involved unless your workspace requires app approval. In that case **Install to Workspace** becomes a request to an admin, and Tomo cannot connect until it is approved.

1. In Tomo, choose **Connect…** for Slack, then **Create the Slack app**. Slack opens with the app's permissions, sign-in address and PKCE filled in from Tomo's manifest; pick your workspace and create it.
2. On **Basic Information**, copy the **Client ID**, and under **App-Level Tokens** generate a token with the `connections:write` scope (it starts with `xapp-`). Paste both into Tomo.
3. To use more than one workspace, open **Manage Distribution** and activate public distribution. The app stays unlisted; only someone with its install link can add it.
4. Choose **Sign in with Slack** and approve the app in the browser. For each further workspace, choose **Add…** under Slack and sign in again.

Sign-in uses PKCE with the redirect URL `tomo://slack/callback`, so Tomo needs no client secret. Slack's app settings accept only an `https` address or a custom scheme, and a custom scheme requires PKCE; turning PKCE on cannot be undone without Slack support. An app created before this needs PKCE turned on and that redirect URL added under **OAuth & Permissions**. macOS hands `tomo://` links only to an installed Tomo app, so Slack sign-in works only from a built app (`pnpm tauri build`), not under `pnpm desktop:dev`. Slack always rotates tokens from a custom-scheme sign-in: Tomo renews them every 12 hours while it runs, and a workspace left unrenewed for 30 days needs signing in again.

The app asks for user scopes only: `im:history`, `im:read`, `im:write`, `mpim:history`, `mpim:read`, `mpim:write`, `channels:history`, `groups:history`, `chat:write`, `files:write`, `users:read`, `channels:read`, `groups:read` and `search:read`. `channels:read` and `groups:read` let Tomo find a channel by name when one of your rules asks it to post there; `search:read` lets it catch up on what arrived while it was closed. It uses Socket Mode and subscribes to message events on your behalf, so Slack pushes new messages to Tomo over a connection Tomo opens; nothing needs a public address. Tomo keeps one connection per app and files each event under the workspace it came from, because Slack delivers an app's events to only one of its connections.

A workspace can still be connected the earlier way, by pasting a User OAuth Token (`xoxp-`) with **Update token…**; that is shown only while no Slack app is set up for sign-in.

Tomo picks up direct and group direct messages, mentions of you in channels, replies in threads you started, and further replies in threads it has already answered in. It replies as you: in the conversation for direct messages, and in a thread for anything in a channel. Files a plan produced are attached to the reply.

The app-level token is optional. Without it Tomo finds new messages only on the polling interval. That check also runs with the token, to catch up on what arrived while Tomo was closed or offline: one Slack search for messages since the last check, of which Tomo keeps direct messages and mentions of you, leaving out a direct message you have already answered. Replies in your threads that do not mention you are not recovered, and a check reads at most 1,000 messages, so after a very long gap in a busy workspace the oldest are left out.

Automatic replies follow the same setting and the same checks as mail. Unlike mail, Slack has no “written to before” check: any direct message or mention may get an automatic reply.

## ChatGPT

Choose **Continue with ChatGPT**. Tomo implements the official open-source dynamic registration flow:

- A stable, locally stored host identifier and the app name `Tomo` identify the installation.
- First sign-in starts with `dynamic_agent_client`; the issued client ID is saved with the verified account.
- Every attempt has a fresh state, OIDC nonce, and PKCE verifier.
- The callback's state and client ID are checked; the ID token is verified against OpenAI's JWKS, issuer, audience, expiry, and nonce.
- ChatGPT plan usage requires the granted `chatgpt.tokens.use.direct` scope.
- Inference uses the public Responses API with `store: false` and streaming enabled. Interrupted, failed, incomplete, and usage-exhausted responses do not trigger a send.

Select a model from the signed-in account's model catalog. Eligibility, preview limits, and plan usage allowances belong to OpenAI. Do not reuse another application's client ID or undocumented ChatGPT endpoints. See [official registration](https://developers.openai.com/siwc/token-sharing-open-source/sign-in) and [inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference).

## OpenAI API fallback

Select **OpenAI API key**, enter your own key, refresh the model list, and choose a model supporting structured Responses output. The key stays encrypted on this Mac and is never included in exported configuration. API billing is separate from a ChatGPT subscription.

For either AI option, enable **Background AI processing** only after reviewing the privacy disclosure. Automatic replies remain off until separately enabled in Rules.

## Confirm the live workflow

Use disposable test messages in an inbox you control. Enable AI consent, leave auto-send off, and confirm a new message appears without refreshing. Inspect the thread and risk reason, edit a harmless draft, and send it to a test recipient you control. Confirm it appears once in Sent, then restart Tomo and confirm no second copy is sent. Exercise pause, reconnect, disconnect, offline recovery, and exhausted-allowance handling before using an account for routine work.
