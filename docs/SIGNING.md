# Code signing & updates

Two independent signatures are involved. Don't confuse them.

## 1. Tauri updater signature (required for auto-update)

Signs the *update artifacts* so the running app cryptographically verifies an
update came from us before applying it. This is separate from OS code signing.

> **Status: there is no working update channel.** Three separate things are
> missing, and all three must be fixed together — fixing one alone changes
> nothing:
>
> 1. **`pubkey` is the literal placeholder.** This fails *closed*, not open:
>    `tauri-plugin-updater` base64-decodes `pubkey` before installing anything
>    (`updater.rs:1453` in 2.10.1), so a forged update could never be applied.
>    It also means no update can ever be applied.
> 2. **No updater artifacts are produced.** `bundle.createUpdaterArtifacts` is
>    not set in `tauri.conf.json`, so the release build emits installers but no
>    `.sig` files and no `latest.json` manifest — nothing for the endpoint to
>    serve. `TAURI_SIGNING_PRIVATE_KEY` in `release.yml` is currently unused.
> 3. **There is no call site.** Nothing in `src/` imports
>    `@tauri-apps/plugin-updater`, so the app never checks for updates.
>
> There is deliberately **no `active` key** in the config. `active` is a Tauri
> **v1** field; v2's updater `Config` has no such member and silently ignores it
> (see the `Deserialize` impl in the plugin's `config.rs`). Setting
> `"active": false` looks like it disables the updater and does nothing at all —
> don't re-add it expecting it to work.
>
> The endpoint URL stays in the config as the intended destination, but treat it
> as a TODO, not a live channel.

**Setup (do this before the first real release):**

```bash
npm run tauri signer generate -- -w ~/.gitglass/updater.key
```

- The **public key** goes into `src-tauri/tauri.conf.json → plugins.updater.pubkey`
  (currently the placeholder `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY`).
- The **private key** goes into CI as `TAURI_SIGNING_PRIVATE_KEY`
  (+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). **Never** commit it.
- Set `bundle.createUpdaterArtifacts: true` in `tauri.conf.json`, or the build
  produces no `.sig` / `latest.json` and the private key above goes unused.
- Add a call site in the frontend (`@tauri-apps/plugin-updater`); the plugin is
  registered in `lib.rs` and permitted in `capabilities/default.json`, but
  nothing invokes it.

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

**Build note:** the macOS job builds a *universal* binary, which cross-compiles
the Intel half on an Apple Silicon runner. `git2`'s `https` feature uses OpenSSL
on Unix, and the runner has no x86_64 OpenSSL — so `vendored-openssl` is enabled
in `Cargo.toml` to build OpenSSL from source per-architecture. Don't remove it or
the macOS release build breaks (Windows is unaffected; it uses SChannel).

The failure this prevents looks like:

```
error: failed to run custom build command for `openssl-sys`
  Could not find openssl via pkg-config:
  pkg-config has not been configured to support cross-compilation.
  $HOST = aarch64-apple-darwin
  $TARGET = x86_64-apple-darwin
```

It is invisible to a native macOS build — arm64 finds a system OpenSSL fine — so
it only ever surfaced when a `v*` tag triggered the universal release build. CI
now runs `cargo check --target x86_64-apple-darwin` on the macOS lane
(`ci.yml`, "Cross-compile check") to catch it on a pull request instead. If you
see the error above, check that `vendored-openssl` is present in `Cargo.toml`
**and that your change is actually committed** — the config only takes effect on
what CI checks out.

**Unsigned (current):** the `.dmg` builds fine without any Apple account, but
Gatekeeper shows "unidentified developer" / "app is damaged." Users run it by
**right-clicking the app → Open** once (the macOS equivalent of Windows'
SmartScreen "Run anyway").

**Signed + notarized (optional, needs an Apple Developer account, $99/yr):**
set these repo secrets and the workflow notarizes automatically — no code change:
`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`. This removes the Gatekeeper
warning entirely.
