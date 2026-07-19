# GitHub setup (one-time, ~60 seconds)

GitGlass signs in with GitHub's **OAuth device flow** — you never paste a token,
and nothing secret is stored in the app config. But device flow needs a public
**client id** from a GitHub OAuth App that *you* own. The client id is **not a
secret** (device flow uses no client secret), so it's safe to commit.

## 1. Register an OAuth App

1. Go to **GitHub → Settings → Developer settings → OAuth Apps → New OAuth App**
   (<https://github.com/settings/developers>).
2. Fill in:
   - **Application name:** `GitGlass` (or anything)
   - **Homepage URL:** `https://github.com/your/gitglass` (any valid URL)
   - **Authorization callback URL:** `http://localhost` (unused by device flow, but required)
3. Create the app, then on its page **check “Enable Device Flow”** and save.
4. Copy the **Client ID**.

## 2. Give it to GitGlass

Create a `.env` in the project root (copy from `.env.example`):

```
VITE_GITHUB_CLIENT_ID=Iv1.your_client_id_here
```

Restart `npm run app:dev`. That's it — click **Sign in** in the title bar, enter
the code GitGlass shows you on the page it opens, and you're connected.

## What GitGlass asks for

Scope requested: `repo read:user workflow`.
- `read:user` — to show who you're signed in as.
- `repo` — to create repositories, push, open pull requests, and read PR/issue
  status. (GitHub's device flow doesn't offer a narrower scope that still allows
  creating private repos.)
- `workflow` — to push files under `.github/workflows`. GitHub rejects *the
  whole push* without it, and GitGlass writes workflows (Automations).

If you signed in before `workflow` was requested, your stored token predates the
scope: **sign out and sign in again** to grant it. A push that trips this reports
exactly that.

## Where the token lives

Only in the **Windows Credential Manager** (macOS Keychain later), under service
`GitGlass`, account `github-token`. Never in `.env`, never in app config, never
logged. Sign out removes it. See [SECURITY.md](SECURITY.md).

## Who the token is sent to

Only `github.com`, and only over HTTPS. The check parses the URL and compares
the **host** — a substring test would hand the token to `github.com.evil.tld`
or `https://github.com@evil.tld/…`. Cloning a non-GitHub URL still works; it
just proceeds unauthenticated instead. See [SECURITY.md](SECURITY.md).

## Signing your commits (Git identity)

Separate from GitHub sign-in: Git stamps each commit with a name and email, and
a fresh machine has neither. GitGlass asks for them the first time you save and
stores them in your **global** Git config, so it's once per machine, not per
repo. It offers your GitHub login and `<login>@users.noreply.github.com`, which
keeps your real address off public commits. No terminal needed.

## Troubleshooting

- **“GitHub sign-in isn’t configured yet.”** — `VITE_GITHUB_CLIENT_ID` is empty;
  set it and restart.
- **The code expired.** — Codes are short-lived; click **Try again**.
- **Publish/clone fails to authenticate** — make sure you're signed in; GitGlass
  uses the stored token for git transport over https.
