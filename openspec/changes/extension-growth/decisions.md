<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — `net:<host>` names one lower-case DNS name, reached on 443 only

- status:   pending
- task:     1.1
- commit:
- trigger:  The task says "a host name, no scheme, no path, no wildcard" and nothing of case, ports or how the decoder tells a near miss.
- decided:  `Grant::Net(host)`, `uze_contract::is_host_name`: labels of `[a-z0-9-]`, 1–63 bytes, no leading or trailing hyphen, 253 bytes in all; upper case, a port, a scheme, a path or `*` are refused with "`…` is not a host name" and a suggestion. `fetch` lower-cases the URL's host and refuses any port but `:443`, so the host read at consent is the host reached. Bare `net` says it names the host; `secret` says a secret is a preference; `edit:checkout` says it is not offered. The `LATER` table of grants "a later change brings" is gone: none is owed now.
- rejected: A port in the grant (`net:host:8443`): no gallery entry needs one, and it doubles what consent must explain. Accepting upper case and folding it: two spellings of one grant in `extensions.json`.
- blast:    `uze-contract::manifest`, `uze-core::…::manifest::grants`, `grant_meaning`.

## D2 — `fetch` hands `curl` one chosen argument; everything else goes on standard input

- status:   pending
- task:     1.2
- commit:
- trigger:  G1 says "builds `curl` argv"; G2 puts the secret in `curl`'s config on stdin. Where the method, headers and body go, and how the status is read, is not said.
- decided:  argv is `uze_platform::tools::CURL_ARGUMENTS` (`-q` first, `--proto =https`, `--proto-redir =https`, `--tlsv1.2`) then `--silent --show-error --globoff --max-redirs 0 --max-time 9 --write-out "\n%{http_code}" --config - --url <url>`: the URL is the only value a script chose. `request`, every `header` and `data-binary` are quoted lines of the config on stdin, the secret's header among them. No `-L`: a redirect is answered as its 3xx status, and a script that wants the `Location` fetches it, checked like any URL. Methods are `GET` and `POST`; a map body is sent as JSON; `Host`, `Content-Length`, `Transfer-Encoding`, `Connection`, `Proxy-Authorization` and the secret's own header cannot be set by a script; an unknown option (a misspelt `secret`) is refused rather than ignored. The answer is `#{status, body, json?}`, `json` present when the body parses.
- rejected: Headers in argv (`-H`): readable in the process list. Following redirects with `--proto-redir` alone: `curl` re-sends custom headers, the secret's included, to wherever it is sent.
- blast:    `uze-script::fetch`, `uze-application::extensions::fetch`.

## D3 — A request runs confined with the network on and the checkout out of reach; unconfined where the machine cannot confine

- status:   pending
- task:     1.2
- commit:
- trigger:  Programs run today with `network: false`; `fetch` needs it on, and the confinement of a `curl` run is not written anywhere.
- decided:  `program::curl` (the one runner caller, as the architecture suite requires) runs the system's `curl` by path in a scratch directory of its own, with `Confinement { network: true }`: read and execute the system prefixes, read the resolver's files where their links lead (`/etc/resolv.conf` → `/run/systemd/resolve/…`) and the CA bundle the environment names, write only its scratch; nothing of the checkout. Beyond the variables every program keeps, it keeps the proxy and CA variables `tools::curl` keeps for the updater. Where the machine does not confine, it runs unconfined: what runs is the system's `curl`, not the extension's code, and the host checked every value it was handed. Checked by hand on WSL: a socket opens with network on and is refused without it.
- rejected: Refusing `fetch` where the machine does not confine: macOS and Windows users would lose the gallery entry for no gain in what runs.
- blast:    `uze-application::extensions::program`.

## D4 — A check reaches no host

- status:   pending
- task:     1.2
- commit:
- trigger:  `uze agent plugin check --run` rehearses an author's scripts; with `fetch`, a check would make real requests.
- decided:  A rehearsal's reach has no fetcher: `fetch` there throws "a check reaches no host; its recorded answers stand in", as a program source is not run where the machine does not confine. A gallery entry's tests answer from fixtures (recorded responses).
- rejected: Rehearsing against the network: a check that depends on a forge being up, and on a token the author may not want used.
- blast:    `uze-application::extensions::reach`.

## D5 — `uze_platform::secrets` keeps a value; `security -i` on macOS, Credential Manager on Windows, a `0600` file elsewhere

- status:   pending
- task:     2.1
- commit:
- trigger:  The task asks for `uze-platform::secrets`; `uze_platform::secret` already exists (unpredictable bytes), and how a value reaches `security` without argv is not said.
- decided:  A new module `secrets` beside `secret`, each saying it is not the other. `SecretKey { service, account, file }` (the file is where the value lives without a keychain; a keychain never writes it). macOS: the system's `security` by path, `add-generic-password -U` written to `security -i` on stdin so the value is in no argv; `find-generic-password -w` reads; exit 44 is "not there". Windows: `CredWriteW`/`CredReadW`/`CredDeleteW` as a generic credential `uze:<account>`, with one feature (`Win32_Security_Credentials`) added to the `windows-sys` already in the tree, no new crate; checked with `cargo clippy --target x86_64-pc-windows-msvc` and `--target aarch64-apple-darwin`, never run. Other Unix: `state/extensions/secrets/<plugin@market>/<id>/<name>` (`UzeHome::extension_secret_path`, record tier), written `0600` in `0700` directories by rename, opened with `O_NOFOLLOW`. A value is one line of printable text, at most 4 KiB.
- rejected: Secret Service on Linux: `zbus` (D18). A `keyring` crate: a dependency the parent design rules out.
- blast:    `uze-platform::secrets`, `UzeHome`, `uze-workspace::extensions::secrets`.

## D6 — Whether a secret is set is kept as `true` among the preferences; the value is read only to send it

- status:   pending
- task:     2.1
- commit:
- trigger:  The Extensions screen says "set"/"not set"; asking the macOS Keychain on every report spawns `security` per secret, and a plugin's removal must find its secrets in a keychain that cannot be listed by prefix.
- decided:  Setting a secret writes the value through `uze_platform::secrets` and `true` under its name in the extension's preferences record; an empty value deletes it and writes `false`. The report reads only the flag. `admitted_preferences` (what `{pref.*}` and `input.preferences` read) leaves every secret out. Removing a plugin deletes, from the keychain, each secret its records flag as set, then the secrets directory.
- rejected: Reading the keychain to answer "set": a process per row per refresh on macOS.
- blast:    `uze-workspace::extensions::{secrets,preferences}`, `uze-application::extensions::{consent,report}`.

## D7 — A secret is scrubbed where it comes back, and a run that sent one keeps nothing

- status:   pending
- task:     2.2
- commit:
- trigger:  G2 scrubs "an answer that echoes it … before it is drawn"; D18 also taints the run (no store patch, no prompt).
- decided:  The host replaces every occurrence of the value in the reply's body and in every failure reason with `[secret]` before the script sees it, so no envelope, trace, toast or log can carry it whole. A run that placed a secret is marked: its answers (a resident session's for the rest of the session) have their `store` patch and `notify` dropped, with a notice saying why. An envelope carries no agent prompt, so nothing else is tainted.
- rejected: Scrubbing only at drawing: the script, its store and its trace would already hold it.
- blast:    `uze-application::extensions::{fetch,runner}`.

## D8 — A `secret` preference is declared with a label and no default, and only beside a `net:` grant

- status:   pending
- task:     2.1
- commit:
- trigger:  The preference kinds declare a `default`; a secret has none an author could ship.
- decided:  `secret "token" label="GitHub token"`: `default` is refused; the check refuses a secret in a manifest that wants no `net:<host>`, since only `fetch` sends one.
- rejected: An empty-string default: it would read as "set" to anything that checks for a value.
- blast:    `uze-core::…::manifest`, `check.rs`.

## D9 — The masked field draws `cursor.masked` per character, and the field opens empty

- status:   pending
- task:     2.1
- commit:
- trigger:  The design system says every glyph is a `Symbol`; no symbol meant "a character not shown".
- decided:  `Symbol::CursorMasked` (`cursor.masked`: `•`, `*` in the ASCII set). Enter on a secret row opens the text prompt empty, whatever is kept, with "Kept in <where> and never shown again. Leave it empty to forget it."; the row then says only `set` or `not set`.
- rejected: Reusing `mark.dot`: a list bullet, not a mask, and a theme free to draw one differently from the other.
- blast:    `uze-theme`, `src/ui/overlay.rs`.

## D10 — `uze status` lists each host under its extension, as it lists programs

- status:   pending
- task:     1.1
- commit:
- trigger:  "consent and `uze status` list the hosts".
- decided:  Consent already lists every grant with its meaning; `net:<host>` reads "sends requests over HTTPS to this host, and to no other". `uze status` adds a `reaches <host>` line under the extension, where its pinned programs are.
- rejected: A column of its own: the standings are three columns and a host is a detail.
- blast:    `src/cli/extensions.rs`, `grant_meaning`.
