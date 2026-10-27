# 05: Distribution, signing, release automation, and repo hygiene

Research date: 2026-10-26. All prices are in USD unless noted, as of 2026-10-26. Sources were accessed on 2026-10-26 (list at the end).

## Summary

1. macOS: the US$99/year Apple Developer Program is the minimum. Without it you cannot notarize. On macOS 15 (Sequoia) and later, an unsigned or un-notarized app is blocked, and users must go to System Settings > Privacy & Security > Open Anyway (Control-click no longer works).
2. Windows: signing no longer buys an instant SmartScreen pass. EV lost that in 2024. Every signed or unsigned new publisher sees "unrecognized app" until reputation builds, which takes "several weeks and hundreds of clean installs" (Microsoft). Only Microsoft Store MSIX avoids this, and Store registration has been free for individuals since 2025-09-10.
3. Windows 11 Smart App Control (when enabled) blocks unsigned executables outright, so an unsigned installer is not a viable public option.
4. Cheapest Windows signing: Azure Artifact Signing at about $9.99/month (individuals in the US and Canada only), Certum Open Source cloud at EUR 49 (worldwide), or SignPath Foundation (free, but the certificate names "SignPath Foundation" as publisher, not you).
5. Homebrew's official cask repo is effectively closed to a new solo project: it needs 225 stars if you submit it yourself, and Homebrew announced on 2025-11-12 that it would disable casks failing Gatekeeper in September 2026. Use a personal tap.
6. GitHub Actions is free on standard runners for public repos, including macOS and Windows, so CI costs nothing. Use Tauri's updater (mandatory signature) or electron-updater (macOS update requires a signed app).
7. Polished path: about $219/year (Apple $99 + Artifact Signing about $120). Cheapest acceptable: $99/year (Apple + free Store MSIX or SignPath).
8. Biggest risk: Windows first-run trust. Even after you pay for signing, new users see SmartScreen warnings for weeks.
9. For hiring managers, the highest-impact items are a README with a screenshot or GIF and per-platform install steps, a clean history of small commits, tests with a CI badge, enforced lint/format, and a LICENSE.
10. Realistic timeline: 10 to 16 weeks part-time from empty repo to a signed v1.0 on both platforms. Signing, notarization, and store onboarding take 2 to 3 weeks of that and should start in week 1, because identity checks and the first notarization have unpredictable lead times.

---

## macOS

### Apple Developer Program
- Cost: 99 USD per membership year, or the local-currency equivalent (Apple "Choosing a Membership"). Individual and organization tiers cost the same. Organizations need a free D-U-N-S number. Fee waivers apply only to nonprofits, education, and government, not to individuals.
- It gives you the Developer ID Application certificate (for signing apps distributed outside the Mac App Store) and access to the notary service. Tauri's docs say a free Apple account can sign for local testing only and "cannot notarize", so the app still shows as unverified.

### Code signing and notarization process
1. Create a "Developer ID Application" certificate in the developer portal. Export it as a `.p12` and store it base64-encoded as a CI secret (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` in Tauri).
2. Sign with the hardened runtime and a secure timestamp (`codesign --options runtime --timestamp`). Tauri and electron-builder do this for you.
3. Notarize with `xcrun notarytool submit <zip|dmg> --wait`. Authenticate with an App Store Connect API key (preferred in CI: `APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH`) or an Apple ID plus app-specific password plus team ID.
4. Staple the ticket (`xcrun stapler staple`) so Gatekeeper can verify offline.
- Tauri docs: "Notarization is required when using a Developer ID Application certificate."
- Typical time: Apple publishes no SLA. Apple DTS says most uploads are "notarised quickly", but some are "held for in-depth analysis." Developer Forums threads from January, March, and June 2026 report first-time submissions stuck "In Progress" for 24 to 48+ hours. Plan for minutes in steady state and up to 2 days for the very first submission. CI implication: put a timeout on `--wait`. If it expires, re-run the job or staple in a follow-up job instead of failing the release.

### Gatekeeper behaviour (what users see)
- Unsigned or un-notarized download on current macOS: the alert reads "Apple cannot check 'Example App' for malicious software" with buttons "Move to Trash" and "Done" (Apple Support 102445). There is no Open button.
- Since macOS 15 Sequoia (announced 2024-08), Control-click > Open no longer overrides Gatekeeper. The user must open System Settings > Privacy & Security, scroll down, click "Open Anyway", confirm again, and enter an admin password. After that the app is remembered as an exception.
- Ad-hoc signing (`signingIdentity: "-"` in Tauri) lets the binary run on Apple Silicon but does not remove the Privacy & Security step.
- Apps downloaded with `curl` or built from source do not get the quarantine attribute, so developers on HN rarely notice the problem. Ordinary users and hiring managers who click a DMG link do.

### Is unsigned acceptable for a portfolio project?
- It is technically possible, and many hobby repos ship unsigned DMGs with a "right-click > Open" note in the README. That note has been wrong since Sequoia.
- For this project's stated goal ("real users can install it painlessly"), it is not acceptable. A hiring manager on a Mac who hits "Move to Trash / Done" and must dig through System Settings gets a poor first impression, and Homebrew distribution needs notarization anyway (next section). Recommendation: pay the $99 from week 1. If the budget is zero for the first months, ship unsigned builds clearly labelled as pre-release, with accurate Sequoia instructions, and sign starting with v1.0.

### Homebrew cask
- Notability (Package Acceptance Policy, shared by formulae and casks): "at least 30 forks, 30 watchers or 75 stars". For a self-submission by the repository owner, the bar is "at least 90 forks, 90 watchers or 225 stars". Repositories under 30 days old are normally ineligible.
- Signing: Acceptable Casks says apps "must pass Homebrew's Gatekeeper checks" and "must not require System Integrity Protection or Gatekeeper to be disabled or bypassed." Homebrew 5.0.0 (2025-11-12) deprecated casks without code signing, deprecated the `--no-quarantine` and `--quarantine` flags, and announced that casks failing Gatekeeper would be disabled in September 2026 (2026-09-01 per a quoted maintainer post). I found no post-deadline confirmation of how this was carried out, so treat it as announced policy.
- Realistic path: a personal tap (`<user>/homebrew-tap` repo, so users run `brew install --cask <user>/tap/<app>`). Taps have no notability bar. The app should still be notarized, because users can no longer easily strip quarantine. Submit to homebrew/cask only after organic stars pass 75 and someone else submits it, or 225 if you submit it yourself.

### Universal binaries (arm64 + x86_64)
- Tauri: `tauri build --target universal-apple-darwin` produces one universal `.app`/`.dmg` (requires both `aarch64-apple-darwin` and `x86_64-apple-darwin` Rust targets installed). Tauri's own CI example builds the two architectures as separate artifacts instead. Either approach works. A single universal DMG makes the README simpler ("download the .dmg").
- Electron: electron-builder supports `--universal` (merges two builds, roughly doubling app size).
- Intel Macs are a shrinking share, and Homebrew is already deprecating Rosetta-only casks (expected to be disabled after macOS 28 ships), but a universal build costs only CI minutes, which are free for public repos.

---

## Windows

### SmartScreen: what users see and how reputation builds
- Unsigned: "Windows protected your PC". The user must click "More info", then "Run anyway". Enterprise policy can block this entirely. Self-signed gets the same treatment as unsigned (Microsoft Learn, SmartScreen reputation, updated 2026-08-17).
- Signed with OV, EV, or Artifact Signing: still flagged as "unrecognized" on first downloads, but the verified publisher name is displayed. Reputation builds from the file hash and the signing identity. Microsoft: "There is no exact threshold, but it can take several weeks and hundreds of clean installs from a wide audience." There is no consumer submission mechanism to speed this up.
- Consistency matters: signing every release with the same publisher identity lets later releases inherit reputation. Unsigned builds start from zero on every update.
- EV no longer bypasses SmartScreen (changed in 2024). Microsoft says paying the EV premium "solely to avoid SmartScreen warnings is no longer justified."
- Smart App Control (Windows 11): when enabled, "Smart App Control will block execution of unsigned files unless the file has a positive reputation", and this applies to all executables, not just downloads. Microsoft says Smart App Control "may supersede" SmartScreen. It is generally reported to be enabled only on clean Windows 11 installs (not verified against a primary source in this research), and where it is on, unsigned binaries without reputation do not run.

### Code signing options and costs (as of 2026-10-26)
| Option | Cost | Eligibility | Notes |
|---|---|---|---|
| Microsoft Store (MSIX) | Free (individual registration fee of $19 removed on 2025-09-10; company fee of $99 removed in 2026-05) | Worldwide, ID + selfie verification | Microsoft re-signs the package: no SmartScreen warning at all. Best Windows trust path. |
| Microsoft Store (MSI/EXE) | You must sign with a CA cert | Worldwide | Store install avoids SmartScreen, but you still need a cert. |
| Azure Artifact Signing (formerly Trusted Signing) | About $9.99/month Basic (5,000 signatures/month); Premium for 100,000/month. The pricing page showed placeholder prices on 2026-10-26, so the figure comes from the Microsoft Learn comparison page dated 2026-08-29 | Individuals: USA and Canada only. Organizations: USA, Canada, EU, UK | No hardware token. Works in GitHub Actions via OIDC federated credentials. Identity comes from an Azure billing account of type Individual. Validation is portal-only (ID + selfie via a third-party verifier), taking hours to a few business days. Billing starts when the account is created. |
| Certum Open Source Code Signing (cloud, SimplySign) | EUR 49 (card set EUR 69, renewal code EUR 25) | Individuals, worldwide, open-source projects | Cheapest worldwide CA option. Cloud signing in CI needs SimplySign tooling and is more fiddly than Azure. Issued to you as an individual. One GitHub thread questioned whether the OSS certificate supports Authenticode, so confirm with Certum before buying. |
| OV certificate (DigiCert, Sectigo, GlobalSign, Certum Standard) | $150 to $300/year (Certum Standard cloud EUR 209) | Worldwide | Since 2023-06, keys must live on an HSM or token, so CI needs a cloud HSM. |
| EV certificate | $400+/year (Certum EV cloud EUR 379) | Worldwide, organizations | Not worth it for SmartScreen any more. |
| SignPath Foundation | Free | OSI license, no proprietary components, actively maintained, already released, functionality documented | Certificate publisher is "SignPath Foundation", not you. Requires a "Code signing policy" section on the project homepage and download page, MFA for all team members, defined author/reviewer/approver roles, manual approval of every release, and fully automated builds verifiable against the repo. Anecdotally they want some existing user base. |
| Self-signed | Free | n/a | Not an option: treated like unsigned, and blocked by Smart App Control. |

- Certificate lifetime: since 2026-03-01 (CA/B Forum ballot CSC-31), publicly trusted code signing certificates last at most 460 days, so multi-year purchases need re-issuance. Artifact Signing rotates short-lived certificates automatically, which is another reason it fits CI.
- Recommendation: if the developer is a US or Canadian resident, use Artifact Signing (the publisher shows as their own name, and it integrates with Actions). Otherwise use Certum Open Source cloud. Also publish MSIX to the Microsoft Store for the zero-warning install path. Consider SignPath only if $0 is a hard requirement and showing "SignPath Foundation" as publisher is acceptable.

### Installer formats
| Format | Pros | Cons | winget? |
|---|---|---|---|
| NSIS `-setup.exe` | Tauri and electron-builder default. Per-user install with no admin rights. Can be cross-built. Works with both updaters. | Needs explicit silent switches in the winget manifest (`/S`). | Yes ("NULLSOFT") |
| MSI (WiX) | Enterprise-friendly, GPO deployable | Tauri can build it only on Windows (WiX v3, needs VBSCRIPT feature). Upgrade semantics are stricter. | Yes ("MSI"/"WIX") |
| MSIX | Clean install/uninstall, Store re-signing, auto-update through the Store | Tauri v2's bundler does not emit MSIX natively (needs MSIX Packaging Tool or `makeappx` as an extra step). electron-builder has an `appx` target. Sideloaded MSIX must be signed with a trusted cert. | Yes ("MSIX"/"APPX") |
| Portable zip | No installer | No Start menu entry or file associations | Yes ("ZIP"/"PORTABLE") |

winget supports EXE (with Silent switches), ZIP, INNO, NULLSOFT, MSI, WIX, APPX, MSIX, BURN, PORTABLE, and FONT. Recommendation: NSIS as the primary download and winget target, plus MSIX for the Store.

### winget submission
- Write a manifest (YAML, versioned folder `manifests/<letter>/<Publisher>/<App>/<version>`). `wingetcreate new <installer-url>` generates it, and `winget validate` checks it locally. Optionally test with `Tools/SandboxTest.ps1`. Open a PR to `microsoft/winget-pkgs`, one package version per PR.
- An automated pipeline then checks the schema, URL reputation, the installer hash, a Defender scan plus several third-party AV engines, silent install and uninstall for admin and non-admin users, and a domain check (`InstallerUrl` must come directly from the publisher's release location; GitHub Releases URLs are fine, redirectors are not). After that a moderator reviews it. If you do not respond to `Needs-Author-Feedback` within 10 days, the PR is closed.
- winget has no star or popularity threshold and does not require signing, but unsigned installers are more likely to hit AV false positives. Automate later version bumps with `vedantmgoyal9/winget-releaser` or `wingetcreate update` in the release workflow.

### Scoop
- The realistic route is a personal bucket (a repo with JSON manifests, so users run `scoop bucket add <name> <url>`). The `extras` bucket has no published numeric threshold, and its maintainers favour established GUI apps. Scoop prefers portable zips, so ship a portable zip artifact too if you want Scoop. It is low priority: winget reaches more Windows 11 users by default.

### WebView2 (Tauri or Wails)
- Windows 11 and Windows 10 (April 2018 and later) ship WebView2 "as part of the operating system", so for this audience the default `downloadBootstrapper` (+0 MB) is right. `embedBootstrapper` adds about 1.8 MB, `offlineInstaller` about 127 MB, `fixedVersion` about 180 MB (you patch it yourself). Avoid `skip`. Set `minimumWebview2Version` if you depend on newer APIs. Wails has equivalent options.

---

## Release automation

### CI matrix (GitHub Actions)
- Cost: free. "GitHub Actions usage is free for self-hosted runners and for public repositories that use standard GitHub-hosted runners." Larger runners are always billed, so avoid them.
- Matrix: `macos-latest` (arm64; build `universal-apple-darwin` or both arches), `windows-latest` (x64; add `aarch64-pc-windows-msvc` later if wanted), plus optionally `ubuntu-latest` for lint and unit tests (cheapest and fastest). For Tauri, `tauri-apps/tauri-action` builds, signs, uploads to a draft release, and generates `latest.json` for the updater. For Electron, `electron-builder --publish always` does the same.
- Triggers: `pull_request` runs lint + test + build (no signing secrets, which forks cannot read anyway). Pushing a tag `v*.*.*`, or merging a release-please PR, runs the signed release build.
- Secrets: Apple `.p12` + password + API key; Azure OIDC federated credential (no stored client secret); updater private key. Scope them to a `release` environment with required reviewers so a compromised PR cannot sign.

### Artifact naming
Use predictable, sortable, platform-explicit names, because winget, Homebrew, and the README link to them:
- `AppName_1.2.3_universal.dmg`, `AppName_1.2.3_x64-setup.exe`, `AppName_1.2.3_x64_en-US.msi`, `AppName_1.2.3_x64.msix`, `AppName_1.2.3_x64_portable.zip`
- Updater files: `AppName_universal.app.tar.gz` + `.sig`, `latest.json` (Tauri) or `latest.yml` / `latest-mac.yml` (electron-updater)
- `SHA256SUMS.txt` and GitHub artifact attestations for every binary.

### Changelog and versioning
- Semantic Versioning: 0.x while the format is unstable, 1.0.0 when install, update, and file associations work on both platforms.
- Conventional Commits (`feat:`, `fix:`, `chore:`), enforced by a commitlint check on PR titles with squash-merge.
- Tools:
  - release-please (Google): reads Conventional Commits and keeps an open "release PR" with the version bump and CHANGELOG.md. Merging it tags and creates the GitHub Release. Best fit for a solo dev, and the release PRs themselves show reviewers a disciplined process.
  - git-cliff: changelog generation only, highly templatable. Pair it with a manual tag.
  - changesets: per-PR changeset files, aimed at JS monorepos. Overkill here.

### Auto-update
| Mechanism | Signing requirement | Notes |
|---|---|---|
| Tauri updater plugin | Update signature is mandatory: "This cannot be disabled." Generate a keypair with `tauri signer generate`, put the public key in `tauri.conf.json`, and put the private key in `TAURI_SIGNING_PRIVATE_KEY` (CI secret; `.env` files do not work). | Static `latest.json` on GitHub Releases works. Updates use NSIS/MSI on Windows and `.app.tar.gz` on macOS. This is separate from OS code signing: you still need Developer ID for Gatekeeper. Losing the private key means existing installs can never update, so back it up offline. |
| electron-updater | macOS: "the application must be signed in order for auto updating to work" (Squirrel.Mac), and needs the `zip` target. Windows: verifies the publisher signature of the new installer against the installed one. | GitHub provider reads `latest.yml` / `latest-mac.yml` from Releases. |
| "Check GitHub Releases" prompt | None | Call the GitHub releases API (`/repos/{owner}/{repo}/releases/latest`) on launch, compare semver, show "Update available" with a link. No silent install. Simplest, privacy-friendly (no telemetry), and a reasonable v0.x choice. |
| Microsoft Store / winget / Homebrew | Handled by the channel | Users who install through package managers update there. Detect this and suppress the in-app updater to avoid conflicts. |

Recommendation: v0.x uses the "check GitHub Releases" prompt. v1.0 uses the Tauri updater (or electron-updater) with signed artifacts.

### Reproducibility, provenance, and dependency hygiene
- Provenance: `actions/attest` (formerly `attest-build-provenance`) is available on all plans for public repos and gives SLSA v1.0 Build Level 2 via Sigstore. Users verify with `gh attestation verify`. This is cheap and impressive in a portfolio.
- Reproducible builds: do not promise bit-for-bit reproducibility for a signed GUI app (signatures, timestamps, and notarization tickets differ per build). Promise "built only in public CI from a tagged commit, with attestations".
- Pinning: commit lockfiles (`Cargo.lock`, `package-lock.json` / `pnpm-lock.yaml`). Pin third-party Actions to a full commit SHA with a version comment. Pin toolchains (`rust-toolchain.toml`, `.nvmrc` / `packageManager`).
- Updates: Dependabot (built in, free; `dependabot.yml` for cargo, npm, and github-actions; enable security updates) or Renovate (more configurable grouping and automerge). For a solo dev, Dependabot with grouped weekly updates is enough. Add CodeQL or `cargo audit` / `npm audit` in CI.

---

## Repo hygiene checklist (ranked by impact on a hiring manager)

Evidence base: first-hand reviewer comments on Hacker News ("Ask HN: What do you look for in an applicant's GitHub?", 2018; "What do recruiters look for in a GitHub profile?", 2019; both dated but still the best first-hand sources found), Built In employer interviews, Blind threads, and career guides (secondary). The common pattern is that reviewers spend about 60 seconds on pinned repos, read the README, skim code style and commit messages, and look for tests. No first-hand reviewer mentioned CODE_OF_CONDUCT, SECURITY.md, or issue templates. Those are ranked lower because they signal maturity mostly to senior OSS-literate engineers.

| Rank | Item | Why |
|---|---|---|
| 1 | README: one-line problem statement, hero screenshot or GIF above the fold, per-platform install (winget / brew tap / DMG / setup.exe / Store), feature list, "why not X" comparison, build-from-source steps | It is the first and often only thing reviewed. Reviewers explicitly check for it. |
| 2 | Clean history of small, focused commits and PRs with good messages (Conventional Commits, squash-merged PRs even when solo) | Cited by HN reviewers. A single giant commit is called out as a negative signal. |
| 3 | Tests + CI badge (unit tests for rendering and sanitization, plus at least one end-to-end smoke test per OS) | "Tests written" is a top signal for engineers. A green badge shows it in one glance. |
| 4 | Lint and format enforced in CI (rustfmt/clippy, eslint/prettier, or equivalent) and an `.editorconfig` | "Following lint/style guides... reveal experience quickly." |
| 5 | LICENSE present and deliberately chosen | Reviewers mention "awareness of licensing". SignPath needs an OSI license. |
| 6 | Working signed releases + CHANGELOG.md + semver tags | Shows you shipped to real users, which is ownership. The release-please PR trail is visible evidence of process. |
| 7 | ARCHITECTURE.md (short codemap, invariants, layer boundaries; per matklad's model) | Lets a senior engineer judge design thinking in 2 minutes. |
| 8 | ADRs in `docs/adr/` (e.g. "0001: Tauri over Electron", "0002: no telemetry", "0003: markdown engine choice") | Shows the reasoning behind decisions, which employers say matters as much as the output. |
| 9 | Demo GIF made with vhs (scriptable, reproducible, in CI) or Kap (macOS screen recorder); keep it under about 5 MB, or use MP4 via GitHub's video embed | Makes item 1 land. |
| 10 | Security: SECURITY.md (private vulnerability reporting enabled), Dependabot, CodeQL, attestations | Especially relevant because a markdown viewer renders untrusted input (XSS in the webview). |
| 11 | CONTRIBUTING.md, issue forms (bug/feature), PR template, CODE_OF_CONDUCT.md (Contributor Covenant) | Cheap to add, and GitHub's community profile checklist counts them. |
| 12 | Roadmap (GitHub Project board or ROADMAP.md) and labelled "good first issue" items | Shows product thinking. |
| 13 | Repo metadata: description, topics, social preview image, website link, pinned on profile, profile README | Affects discoverability and the 60-second scan. |

### License choice
- MIT: shortest and most familiar, with maximum adoption. It has no explicit patent grant.
- Apache-2.0: permissive plus an explicit patent grant and a NOTICE convention. Slightly heavier, and preferred by some companies' legal teams.
- MPL-2.0: file-level copyleft (changes to your files must be shared, but the app can be combined with anything). Signals "contributions should flow back" without GPL friction.
- Recommendation: MIT, or the Rust-ecosystem convention "MIT OR Apache-2.0" (dual), which is what Tauri itself uses. Both are OSI-approved (SignPath-eligible) and frictionless for employers who might read or reuse the code. Avoid GPL for a portfolio piece unless copyleft is a deliberate stance, and record the choice in an ADR.

---

## Metrics and marketing basics
- Landing page: GitHub Pages (free) on `<user>.github.io/<app>` or a custom domain (about $12/year), built from `/docs` or a `gh-pages` branch. Include the hero GIF, an OS-detecting download button pointing at the latest release, install commands, and the "Code signing policy" section if using SignPath.
- Download metrics without telemetry: the GitHub Releases API reports a `download_count` per asset. winget and Homebrew analytics (Homebrew publishes install counts for official taps only) add more. Stars and traffic are under Insights > Traffic (14-day window, so snapshot them weekly).
- Launch channels (all free, self-serve):
  - Show HN: must be something people can try with no signup. Landing pages alone do not qualify. Title starts with "Show HN:". Do not solicit upvotes. Post once v1.0 installs cleanly on both OSes.
  - AlternativeTo: add the app as an alternative to Typora, MarkText, Marked 2, QuickLook plugins, etc. This drives steady long-tail traffic.
  - Product Hunt: free to launch. It is weighted toward consumer and maker audiences, so it is optional.
  - Also: r/macapps, r/windows, r/opensource, the awesome-markdown lists.
- Telemetry: recommend none. Reasons: (1) a read-only viewer has no product need for it; (2) a "no network calls except the optional update check" promise is a selling point and is easy to verify in an open-source repo; (3) telemetry triggers GDPR/consent work, a privacy policy, and SignPath's privacy-statement requirement; (4) network activity on first launch increases AV/SmartScreen suspicion and user distrust; (5) GitHub download counts and issues give enough signal. Record this in ADR "0002: no telemetry". If you ever add it, make it opt-in.

---

## Cost table (as of 2026-10-26)

| Item | One-time | Yearly | Required? |
|---|---|---|---|
| Apple Developer Program | 0 | $99 | Yes, for notarization |
| Azure Artifact Signing (Basic, US/CA individuals) | 0 | about $120 ($9.99/month) | One of the Windows signing options |
| Certum Open Source cloud (non-US/CA alternative) | EUR 49 first year (renewal not confirmed; likely similar) | about EUR 49 | Alternative |
| SignPath Foundation | 0 | 0 | Alternative (publisher is "SignPath Foundation") |
| Traditional OV cert | 0 | $150 to $300 | Not recommended unless forced |
| EV cert | 0 | $400+ | Not recommended |
| Microsoft Store individual account | 0 | 0 | Recommended (zero-SmartScreen MSIX path) |
| GitHub (public repo, Actions on standard runners, Pages, attestations, Dependabot, CodeQL) | 0 | 0 | Yes |
| winget, Scoop bucket, Homebrew tap | 0 | 0 | Yes |
| Custom domain for landing page | 0 | about $12 | Optional |
| Mac for local testing (if the dev only owns Windows) | $0 (CI-only) to $600+ (used Mac mini) | 0 | Strongly advised for UX testing. CI can build and sign without one. |

## Minimal-cost path versus polished path

### Minimal acceptable path: about $99/year
- macOS: Apple Developer Program ($99), Developer ID signing + notarization in CI, universal DMG, personal Homebrew tap.
- Windows: unsigned NSIS on GitHub Releases is not acceptable because of Smart App Control. Instead use one of:
  - (a) Microsoft Store MSIX (free, no SmartScreen, but Tauri needs an extra MSIX packaging step), plus winget pointing at the Store ID; or
  - (b) SignPath Foundation (free) signing for NSIS, accepting "SignPath Foundation" as publisher, per-release manual approval, and the risk that a brand-new project is not yet eligible ("already released", "actively maintained").
- Updates: "check GitHub Releases" prompt.
- If even $99 is unavailable at first, ship v0.x unsigned on macOS with correct Sequoia instructions, labelled "pre-release", and buy the membership before v1.0 and before any HN launch.

### Polished path: about $219/year (+ optional $12 domain)
- macOS: as above, plus a notarized universal DMG and a personal Homebrew tap updated automatically from the release workflow.
- Windows: Azure Artifact Signing ($9.99/month; US/CA individual) signing NSIS + MSI in GitHub Actions with OIDC. Non-US/CA developers substitute Certum OSS cloud (EUR 49). Also publish to the Microsoft Store (MSIX, free) and winget (automated PR on release).
- Updates: Tauri updater (or electron-updater) with a minisign key, `latest.json` on Releases, and in-app updating turned off for Store, winget, and brew installs.
- Process: release-please, Conventional Commits, attestations + SHA256SUMS, Dependabot, CodeQL, environment-protected signing secrets.
- Repo: all 13 checklist items, a GitHub Pages landing page, and vhs-generated GIFs rebuilt in CI.

## Realistic timeline (solo, part-time about 10 to 15 h/week)

| Weeks | Work | Distribution-specific notes |
|---|---|---|
| 1 | Repo skeleton, LICENSE, README stub, CI (lint + test on mac + win), ADR 0001 | Start Apple enrollment (identity checks can take days) and Azure identity validation or a SignPath/Certum application now. They have unpredictable lead times. |
| 2 to 6 | Core viewer features, tests, ARCHITECTURE.md | Unsigned nightly artifacts from CI |
| 7 to 8 | Signing + notarization in CI on both OSes, artifact naming, release-please, attestations | Do the first notarization early. Expect a possible 24 to 48 h first-time hold. |
| 9 to 10 | Updater, Homebrew tap, winget manifest, Store MSIX submission | winget PR review: days. Store certification: typically a few business days. |
| 11 to 12 | Docs polish, GIFs, landing page, SECURITY/CONTRIBUTING/templates, beta testers | SmartScreen reputation starts accumulating from the first signed beta. Ask beta testers to install from the real download URL. |
| 13 to 16 | Buffer for signing, notarization, and AV false-positive surprises; v1.0; Show HN | Launch only after a clean install on a fresh Windows 11 VM (Smart App Control on) and a fresh macOS VM. |

Distribution work (signing, notarization, packaging, store and package-manager submissions, updater) is realistically 40 to 60 hours of the total, much of it waiting on third parties.

## Biggest distribution risk
Windows first-run trust. Even with a paid certificate or Artifact Signing, a new publisher's installer shows a SmartScreen "unrecognized app" prompt for "several weeks and hundreds of clean installs", and Smart App Control can block reputation-less binaries entirely. A portfolio app may never reach that volume. Mitigations: sign every release with one stable identity from the first beta, publish MSIX through the free Microsoft Store (the only zero-warning path), and document the prompt honestly in the README.

Secondary risks: the first Apple notarization being held for 1 to 2 days right before a launch; losing the updater private key (existing users stranded); AV false positives blocking the winget PR; Azure individual eligibility being limited to the US and Canada.

---

## Sources (all accessed 2026-10-26)

Apple
- Apple, Choosing a Membership: https://developer.apple.com/support/compare-memberships
- Apple, Notarizing macOS software before distribution: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution
- Apple, Customizing the notarization workflow: https://developer.apple.com/documentation/security/customizing-the-notarization-workflow
- Apple Support, Safely open apps on your Mac (102445): https://support.apple.com/en-us/102445
- Apple Developer Forums, notarization stuck "In Progress" threads: https://developer.apple.com/forums/thread/813169 , https://developer.apple.com/forums/thread/818292 , https://developer.apple.com/forums/thread/835838 , https://developer.apple.com/forums/thread/780865
- Cult of Mac, macOS Sequoia removes Control-click Gatekeeper bypass: https://www.cultofmac.com/864843/macos-sequoia-removes-handy-shortcut-bypass-gatekeeper-security/
- idownloadblog, Sequoia Gatekeeper change: https://www.idownloadblog.com/apple-macos-sequoia-gatekeeper-change-install-unsigned-apps-mac/

Homebrew
- Homebrew, Acceptable Casks: https://docs.brew.sh/Acceptable-Casks
- Homebrew, Package Acceptance Policy: https://docs.brew.sh/Package-Acceptance-Policy
- Homebrew 5.0.0 release post (2025-11-12): https://brew.sh/2025/11/12/homebrew-5.0.0
- Workbrew, What Homebrew 5.0.0 means: https://workbrew.com/blog/what-homebrew-5-0-0-means-for-your-mac-fleet

Microsoft / Windows
- Microsoft Learn, Code signing options for Windows app developers (updated 2026-08-29): https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options
- Microsoft Learn, SmartScreen reputation for Windows app developers (updated 2026-08-17): https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- Azure, Artifact Signing pricing: https://azure.microsoft.com/en-us/pricing/details/artifact-signing/
- Microsoft Learn, Artifact Signing quickstart: https://learn.microsoft.com/azure/artifact-signing/quickstart
- Microsoft Learn, Artifact Signing integrations: https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations
- Windows Developer Blog, Free developer registration for individual developers (2025-09-10): https://blogs.windows.com/windowsdeveloper/2025/09/10/free-developer-registration-for-individual-developers-on-microsoft-store/
- Windows Developer Blog, Company accounts free (2026-05-07): https://blogs.windows.com/windowsdeveloper/2026/05/07/publish-to-microsoft-store-as-a-company-now-with-free-registration-and-faster-onboarding/
- Microsoft Learn, WinGet overview and supported installer formats: https://learn.microsoft.com/en-us/windows/package-manager/winget/
- Microsoft Learn, Submit packages to Windows Package Manager: https://learn.microsoft.com/en-us/windows/package-manager/package/
- Microsoft Learn, Submit your manifest to the repository: https://learn.microsoft.com/en-us/windows/package-manager/package/repository
- Scoop, Concepts (buckets): https://scoop.netlify.com/concepts/

Certificates
- Certum shop, Code Signing: https://shop.certum.eu/code-signing.html
- DigiCert, Code signing certificate validity change (CSC-31): https://www.digicert.com/blog/understanding-the-new-code-signing-certificate-validity-change
- GlobalSign, Maximum certificate validity: https://support.globalsign.com/ssl/general-ssl/maximum-certificate-validity
- SignPath Foundation, Terms: https://signpath.org/terms
- SignPath Foundation, Apply: https://signpath.org/apply
- SignPath, Open source product page: https://about.signpath.io/product/open-source

Frameworks and release automation
- Tauri v2, Updater plugin: https://v2.tauri.app/plugin/updater/
- Tauri v2, Windows installer (WebView2 modes, NSIS vs MSI): https://v2.tauri.app/distribute/windows-installer/
- Tauri v2, macOS code signing: https://v2.tauri.app/distribute/sign/macos/
- Tauri v2, Windows code signing: https://v2.tauri.app/distribute/sign/windows/
- electron-builder, Auto update: https://www.electron.build/docs/features/auto-update
- Electron, autoUpdater API: https://electronjs.org/docs/latest/api/auto-updater
- Electron, Code signing: https://www.electronjs.org/docs/latest/tutorial/code-signing
- GitHub Docs, GitHub Actions billing: https://docs.github.com/en/billing/concepts/product-billing/github-actions
- GitHub Docs, Artifact attestations: https://docs.github.com/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds
- GitHub Blog, Introducing Artifact Attestations: https://github.blog/security/supply-chain-security/introducing-artifact-attestations-now-in-public-beta/
- GitHub Docs, Default community health files: https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions/creating-a-default-community-health-file
- pistack, git-cliff vs release-please vs auto (2026-04-22): https://www.pistack.xyz/posts/2026-04-22-git-cliff-vs-release-please-vs-auto-self-hosted-changelog-generator-guide-2026/

Repo hygiene and hiring
- Hacker News, Ask HN: What do you look for in an applicant's GitHub? (2018): https://news.ycombinator.com/item?id=16224401
- Hacker News, Ask HN: What do recruiters look for in a GitHub profile? (2019): https://news.ycombinator.com/item?id=19413348
- Built In, What 4 companies really look for on your GitHub profile: https://builtin.com/articles/what-companies-look-for-on-github
- Built In, What employers are looking to see in your GitHub profile: https://builtin.com/articles/github-advice-job-seeker
- Teamblind, Do recruiters or hiring managers check GitHub: https://www.teamblind.com/post/do-recruiters-or-hiring-managers-check-github-sfrfrpxj
- underdog.io, What hiring managers look for on GitHub: https://underdog.io/blog/what-hiring-managers-look-for-on-github
- Reczee, What do hiring managers see on my GitHub profile: https://www.reczee.com/blog/what-do-hiring-managers-see-on-my-github-profile
- SOLTECH, What hiring managers look for in a GitHub portfolio: https://soltech.net/what-do-hiring-managers-actually-look-for-in-a-github-portfolio/
- ITPro Today (HackerRank 2018 survey): https://itprotoday.com/it-management/developers-show-your-github-project-to-attract-job-offers
- matklad, ARCHITECTURE.md: https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html

Marketing
- Hacker News, Show HN guidelines: https://news.ycombinator.com/showhn.html
- AlternativeTo, Microsoft drops Store fee for individuals (2025-09): https://alternativeto.net/news/2025/9/microsoft-finally-drops-microsoft-store-publishing-fee-for-individual-developers-globally
