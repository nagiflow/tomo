# Releasing Tomo

For maintainers.

## Publish a release

1. In a pull request, set `version` in `src-tauri/tauri.conf.json` (and `package.json`) to the new version, then merge it.
2. Tag the merged commit and push the tag:

   ```sh
   git switch main && git pull
   git tag v0.2.0 && git push origin v0.2.0
   ```

3. The tag runs [`release.yml`](../.github/workflows/release.yml) on a macOS runner, the only workflow that uses one. It builds a universal `.dmg`, signs it with the Developer ID certificate, notarizes and staples both the app and the `.dmg`, and attaches them to a **draft** release. It takes about 45 minutes.
4. Check the draft on the [Releases page](https://github.com/nagiflow/tomo/releases), write the notes, and click **Publish release**. The README's download link always points to the latest published release.

To check a downloaded `.dmg` before publishing:

```sh
spctl -a -t open --context context:primary-signature -vv Tomo_<version>_universal.dmg
# expect: accepted, source=Notarized Developer ID
```

## Repository secrets

`release.yml` reads these. They belong to the nagiflow LLC Apple developer team (`H8D9H857LL`).

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | The "Developer ID Application" certificate and its private key, exported as `.p12` and base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | The password of that `.p12` |
| `KEYCHAIN_PASSWORD` | Any random string, for the temporary keychain on the runner |
| `APPLE_API_KEY` | The Key ID of the App Store Connect API key used to notarize (Users and Access → Integrations) |
| `APPLE_API_ISSUER` | The Issuer ID shown above the keys on that page |
| `APPLE_API_KEY_P8` | The contents of that key's `AuthKey_<KeyID>.p8` file |

To rotate the API key, create a new one with **Developer** access, update the three `APPLE_API_*` secrets, then revoke the old key. To replace the certificate, create a new Developer ID Application certificate in Xcode (Settings → Accounts → Manage Certificates), export only it from Keychain Access as `.p12`, and update the first two secrets:

```sh
base64 -i tomo-devid.p12 | gh secret set APPLE_CERTIFICATE -R nagiflow/tomo
gh secret set APPLE_CERTIFICATE_PASSWORD -R nagiflow/tomo
```
