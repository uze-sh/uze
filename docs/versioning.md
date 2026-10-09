# Versioning

UZE uses [Semantic Versioning 2.0.0](https://semver.org/) from its first
distributable build. Until 1.0.0 itself, every release remains an explicit
pre-release. The alphas ran as `0.0.0-alpha.N`; the series now leading to
1.0.0 is:

```text
1.0.0-beta.N
```

`[workspace.package].version` in the root `Cargo.toml` is the sole version
source for the installable `uze` binary, Core, integrations, Application and
conformance workspace members. Do not version a member independently.

Before producing a binary intended for installation, increment that value:

- `beta.N + 1` for the next delivery on the way to 1.0.0;
- `rc` once 1.0.0 is a candidate (`1.0.0-rc.1`), and `patch`/`minor`/`major`
  only when the project deliberately promotes it to a stable release.

Development builds may be rebuilt freely without a version change. A binary
that is copied, installed, attached to a release, or shared for testing must
carry a newly incremented SemVer pre-release version. Confirm it with:

```bash
make version
make release
target/release/uze --version
```

## Releasing a binary

UZE's official Linux distribution channel is GitHub Releases, consumed by
`install.sh` (`curl -fsSL https://uze.sh/i | sh`).
A release enters `main` through the same door as every other change: a pull
request. No local cargo-release, no manual push, and nothing that a branch
ruleset has to make an exception for.

**1. Propose it.** Run the **Release** workflow (Actions → Release → Run
workflow). The `bump` input defaults to `beta`; use `rc` for a release
candidate and `patch`/`minor`/`major` for deliberate stable milestones. The workflow, on a branch of its own:

   - `cargo release … --execute --no-tag` runs the `make check`
     pre-release-hook (fmt, clippy, cargo-deny, tests, ruff — the gate;
     nothing is bumped if it fails) and bumps every workspace crate in
     lockstep (`shared-version`, see `release.toml`), producing the
     `chore(release): bump workspace version to <v>` commit;
   - `git-cliff -t <v>` regenerates `CHANGELOG.md` — `-t` names the new
     section before the tag physically exists (with the tag already in
     place git-cliff would see an empty range and omit it) — and the
     changelog is folded into the release commit;
   - `git-cliff --config cliff.release.toml` renders the release page
     these commits will publish, and it goes into the pull request body:
     the notes are reviewed with the bump, not discovered afterwards;
   - the branch is pushed and opened as a `chore(release): v<v>` pull
     request. Nothing is tagged and nothing has reached `main`.

**2. Merge it.** The release is reviewed and gated like anything else, and
the merge is the decision to publish.

**3. Publishing happens on that push.** The same workflow, on every `push`
to `main`, reads the version in the tree and asks whether it has a published
*release* yet. When it does — an ordinary change, or a version already out —
it stops there. When it does not:

   - the annotated `v<v>` tag is created on the merge commit, which is the
     commit carrying version bump + changelog + lockfile, or reused when an
     earlier attempt got that far before failing. A `v<v>` tag that names
     any other commit stops the release: `Gate` passed for this commit, so
     this commit is what gets built, and every later job checks it out by
     its hash, never by the tag's name;
   - the six artifacts (Linux `x86_64`/`aarch64` × `gnu`/`musl`, and macOS
     `x86_64`/`aarch64`) are built
     from that tag on native runners and published as
     `uze-<arch>-linux-<libc>.tar.gz` — the Rust target triple without its
     vendor field, because `unknown` is a triple saying there is no vendor
     and a person choosing a download should not have to know that. The
     workflow's matrix and `install.sh` derive that name separately, so the
     two must be changed together;
   - the two Windows archives (`x86_64` and `aarch64`) are built natively in
     the same matrix and published as `uze-<arch>-windows.zip`, the name
     `install.ps1` asks for, with a Scoop manifest, `uze.json`, written from
     their own bytes;
   - a CycloneDX SBOM is generated from the tag's own lockfile, provenance
     is signed for every asset (`gh attestation verify <file> --repo
     uze-sh/uze`), and the GitHub Release — named `v<v>`, the same
     identifier the tag, the changelog and `install.sh` all use — is
     created with the tarballs, the SBOM, `install.sh` and `install.ps1`,
     `SHASUMS256.txt`, its signature `SHASUMS256.txt.sig` (see
     [Signing](#signing)) and the notes described below. A re-run uploads
     only the assets a release is missing and rewrites the notes; it never
     replaces an asset already published (no `--clobber`), and with
     immutable releases on GitHub refuses that anyway.

The eight build jobs run with read-only permissions, no persisted Git
credentials and no build cache, since a cache is written by every pull
request's CI run; only the jobs that push the tag and publish the release
can write, and only `publish` holds the signing key.

Asking about the release rather than the tag is what makes the repair
possible: the first attempt at `v0.0.0-alpha.1` tagged the commit and then
failed to build two of its four targets, and a tag-only check would have
left that version unpublishable for good.

## What the release page says

Two documents come out of the same commits, and they are not the same
document. `CHANGELOG.md` (`cliff.toml`, `make changelog`) is history: every
release, committed, regenerated offline, so the bytes are the same on a
laptop and in CI — which is why nothing in it depends on the GitHub API and
its pull request links are derived from the `(#N)` a squash merge leaves in
the subject.

The release page (`cliff.release.toml`, `make release-notes`) is written for
whoever lands on it from a search result or an install link. It carries only
the current release, and it adds what a page needs and a file does not:
breaking changes called out above everything else, each entry credited to the
handle that wrote it, first-time contributors named, the compare link against
the previous tag, and the install and verification commands for that exact
version. The handles come from `[remote.github]` — with `GITHUB_TOKEN` in the
environment git-cliff resolves each commit to its pull request and author;
without one the notes still render, one degree less generous, which is what
`make release-notes` gives you locally.

Both configurations group commits identically, so a change sits under the
same heading wherever a reader meets it. Merge commits and the release bump
itself are skipped; dependency bumps have a group of their own.

A release is deliberately *not* published with `--prerelease`, even though
every version until 1.0.0 is a pre-release: GitHub keeps pre-releases out of
`releases/latest`, which is the URL `install.sh` downloads from when
`UZE_VERSION` is unset.

Each tarball carries `LICENSE`, `NOTICE` and `CREDITS.md` beside the binary:
Apache-2.0 §4(a) obliges whoever receives the binary to receive the licence
with it.

The version is deliberately absent from the asset name: the default install
resolves `releases/latest/download/<asset>`, a URL that only works when the
filename is the same in every release.

`https://uze.sh/i` serves `install.sh` (and `install.ps1` to PowerShell)
from the latest release's own assets, not from `main`: the installer a
person pipes into a shell is the one that was released with the binary it
installs.

`install.sh` picks the artifact for the host (`uname -s`/`uname -m`, musl
detection via `ldd --version`), verifies the signature over
`SHASUMS256.txt` with `ssh-keygen -Y verify` against the release key it
carries, verifies the SHA-256 against `SHASUMS256.txt`, refuses to install
when either fails, then installs to `$XDG_BIN_HOME` or
`~/.local/bin` (`UZE_BIN_DIR` overrides; `UZE_VERSION` pins a release;
`UZE_BASE_URL` points at a mirror). It opens with the same
centred header `uze --help` does, reports one step at a time — download,
checksum, install, verify — with a spinner on the line in flight and a
check on every line already settled, closes on the two commands worth
running next, and falls back to a plain, escape-free transcript whenever
stdout is not a terminal or `NO_COLOR` is set, which is what CI and the
fixture suite read. That suite is `make test-installer` (also gating CI).

## Signing

`SHASUMS256.txt` says the archives arrived whole; it cannot say they are
uze's, because whoever can publish to the release page publishes the
checksums too. So every release also carries `SHASUMS256.txt.sig`, an
OpenSSH signature (`ssh-keygen -Y sign`, namespace `uze-release`) made with
a key that lives only in the `release` environment's
`UZE_RELEASE_SIGNING_KEY` secret and offline with the maintainer. The
public half is committed once, in `release-signing.pub`: the binary embeds
it when it is built, `install.sh` and `install.ps1` carry the same line,
and a test (`the_installers_carry_the_key_the_binary_is_built_with`) holds
the three equal.

Every reader verifies with the system's own OpenSSH (8.1 or later), so no
cryptography is added to the binary: the updater runs `ssh-keygen` from the
system's directories by path, `install.sh` the one on `PATH`, `install.ps1`
the one under `System32\OpenSSH`. No signature, a signature by any other key
or for any other purpose, or no `ssh-keygen` at all, and nothing is
installed. A machine without OpenSSH's client is told to install it.

Until `release-signing.pub` holds a key, it holds a placeholder: the
installers and the updater refuse every release, and the `publish` job
refuses to publish one. Making the key is the maintainer's, once:

```sh
ssh-keygen -t ed25519 -C uze-release -f uze-release   # set a passphrase for the offline copy
```

The public half (`uze-release.pub`) replaces the placeholder line in
`release-signing.pub` and the empty key in `install.sh`
(`UZE_RELEASE_KEY='…'`) and `install.ps1` (`$ReleaseKey = '…'`). The private
half goes into the secret without a passphrase (CI cannot type one), from a
copy that is deleted afterwards; the passphrase-protected original stays
offline:

```sh
cp uze-release ci-key && ssh-keygen -p -f ci-key -N ''   # asks for the passphrase
gh secret set UZE_RELEASE_SIGNING_KEY --env release < ci-key && shred -u ci-key
```

Rotating the key is the same steps with a new key, released before the old
one is retired: a binary trusts the key it was built with, so a binary
built before the rotation installs nothing signed only by the new key, and
must be reinstalled with `install.sh`.

## Staying current

A binary `install.sh` placed keeps itself current. The installer leaves a
receipt at `~/.uze/state/install.json` naming the file it wrote and the
release it was, and that file is the only one the updater ever replaces: a
`uze` running from anywhere else — `cargo install`, `make install`, a
package manager, a build tree — was put there by something else, and is
left to it.

The terminal workspace checks when it opens and every hour it stays open,
on a thread of its own. A CLI command never touches the network: when the
last answer is more than an hour old it hands the check to a detached
`uze upgrade --background` and exits, and what that finds is what the next command
mentions — once per release, on stderr, and never after `uze agent` or
`terminal`, whose reader is not a person at a prompt.

"Latest" is where `releases/latest` redirects, the same answer the
installer resolves. `SHASUMS256.txt` is verified against the release key
the binary was built with, the archive against `SHASUMS256.txt`, both the
way the installer verifies them, the binary inside is made to report the
release it claims to be, and only then is it renamed over the old file —
beside it, on the same filesystem, so the swap is one rename and a pane's
shim never runs a half-written binary. Anything already running keeps the
binary it started from; the next launch is the first to run the new one,
and both sidebars say so — "restart to use it". Clicking the notice
opens that release's notes in a modal — its own section of the
`CHANGELOG.md` at its tag, fetched with the release and kept in
`~/.uze/cache/release-notes.md` — from which the release page opens. That is the only thing they say about releases: one that
is merely available, or that this binary will not install itself, is left
to `uze upgrade`. The first CLI command after an update mentions it once.

The background check installs a release only once it has been the latest
for a day (`SETTLE` in `src/self_update.rs`): a release found to be bad and
pulled within that day never reaches a machine that was only waiting for
it. `uze upgrade` does not wait.

One consequence is worth knowing before it happens: when a release changes
the terminal protocol, the first client of the new release replaces the
server the old one left running. Tabs are restored and every agent's
conversation resumes (see ADR-047), but a program in the middle of
something in a pane is restarted. That is what any upgrade does today; an
automatic one only changes who started it.

`uze upgrade` does the same replacement now, in the foreground, and reports
it: the version it placed, that it is already current, or — when it
replaces nothing — why. A receipt that names a different file than the
`uze` that ran is the usual reason a curl install seems never to update:
another `uze` earlier on `PATH` is the one being run. `UZE_AUTOUPDATE`
governs only the automatic check; asking is consent enough.

`UZE_AUTOUPDATE=off` stops the check, `notify` checks without replacing,
and `on` is the default — except where `CI` is set, which is off unless the
variable says otherwise. What the updater remembers between runs is in
`~/.uze/state/update.json`.

`UZE_BASE_URL` is the installer's alone. The updater downloads a binary and
renames it over the one in `PATH`, so where it downloads from is a constant
in the binary and not an environment variable: anything that can set one —
a cloned repository's `.envrc`, a `Makefile`, a parent process — would
otherwise choose which `uze` a person runs from then on, and the checksum
could not tell, because `SHASUMS256.txt` is fetched from that same root. A
debug build still honours it, so the offline fixture suite can drive a whole
pass without a network.
