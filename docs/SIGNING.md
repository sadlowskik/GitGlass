# Code signing & updates

Two independent signatures are involved. Don't confuse them.

## 1. Tauri updater signature (required for auto-update)

Signs the *update artifacts* so the running app cryptographically verifies an
update came from us before applying it. This is separate from OS code signing.

**Setup (do this before the first real release):**

```bash
npm run tauri signer generate -- -w ~/.gitglass/updater.key
```

- The **public key** goes into `src-tauri/tauri.conf.json → plugins.updater.pubkey`
  (currently the placeholder `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY`).
- The **private key** goes into CI as `TAURI_SIGNING_PRIVATE_KEY`
  (+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). **Never** commit it.

The Release workflow (`.github/workflows/release.yml`) reads these and produces
a signed `latest.json` manifest served from the updater endpoint.

**Tests (Shippable milestone):** verify a tampered artifact is *rejected* and a
correctly signed one is *accepted* — see the updater signature-verification
tests planned in the roadmap.

## 2. Windows Authenticode (STUBBED)

Signs the `.msi` / `.exe` / `.nsis` installers so Windows SmartScreen trusts
them and the publisher name shows correctly.

**Current state — stubbed on purpose (Milestone 1):**

- `scripts/sign-windows.ps1` runs in **stub mode** when `WINDOWS_CERT_BASE64` is
  unset: it logs what it would sign and exits 0, so dev/CI builds succeed unsigned.
- The pipeline is therefore proven end-to-end *now*, before the cert exists.

**To enable real signing (when you supply the cert):**

1. Export the code-signing cert as a password-protected `.pfx`.
2. Base64-encode it and add repo secrets `WINDOWS_CERT_BASE64` + `WINDOWS_CERT_PASSWORD`.
3. Add to `tauri.conf.json → bundle.windows`:
   ```json
   "signCommand": "powershell -ExecutionPolicy Bypass -File scripts/sign-windows.ps1 -Path %1"
   ```
4. Ensure `signtool.exe` (Windows SDK) is on the runner PATH.

No source change is needed to flip from stub → real; it's driven entirely by the
presence of the secret.

## 3. macOS builds & notarization

**Mac builds only happen on macOS** — you cannot produce a `.app`/`.dmg` from
Windows. The Release workflow (`release.yml`) has a `macos-latest` matrix entry
that builds a **universal `.dmg`** (Apple Silicon + Intel) and attaches it to the
GitHub Release, so Mac users download it there. This runs automatically when you
push a `v*` tag — no Mac hardware required.

**Unsigned (current):** the `.dmg` builds fine without any Apple account, but
Gatekeeper shows "unidentified developer" / "app is damaged." Users run it by
**right-clicking the app → Open** once (the macOS equivalent of Windows'
SmartScreen "Run anyway").

**Signed + notarized (optional, needs an Apple Developer account, $99/yr):**
set these repo secrets and the workflow notarizes automatically — no code change:
`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`. This removes the Gatekeeper
warning entirely.
