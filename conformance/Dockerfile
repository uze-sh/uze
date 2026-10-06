# syntax=docker/dockerfile:1

FROM rust:1.97-bookworm AS uze-build

WORKDIR /src
COPY . .
# Cache mounts make every rebuild incremental: the crate registry and the
# build target persist across builds even when sources change. The binaries
# are copied to /out because cache mounts are not part of the image.
# Two invocations, not one: the fixture is behind `--features dev-servers`,
# which carries rmcp/tokio/schemars — and the `uze` the Lab drives has to be
# the binary a user gets, linking none of them.
# The rustup home is one of them because `rust-toolchain.toml` names a
# channel, not the image's own 1.97: without it every build downloaded and
# unpacked that toolchain again, ~0.7 GB of cache per rebuild.
RUN --mount=type=cache,target=/usr/local/rustup \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release --bin uze \
    && cargo build --locked --release --features dev-servers --bin uze-mcp-conformance-fixture \
    && mkdir -p /out \
    && cp /src/target/release/uze /src/target/release/uze-mcp-conformance-fixture /out/

FROM node:22-bookworm-slim AS harness-lab

# Latest-channel policy: the harnesses update on their own; the Lab always
# tests what the channel delivers. Actual versions are NOT asserted here —
# they are recorded per run in the L2 evidence (each harness's own
# version probe, scenario R6).
# ARG PNPM_VERSION=10.0.0  # (reserved; see note below)

# `jq` is the generated hook wrapper's own dependency (ADR-033): the file a
# harness executes reads the payload with it. The Lab installs it the way a
# machine running delivered hooks must have it, and the wrapper's
# missing-dependency behaviour is proven in the deterministic suite.
# `procps` is Codex's: since 0.157.0 an interactive launch starts the shared
# app-server daemon by default, which reads its process's start time with
# `ps` and exits the TUI without one. Every machine it ships to has `ps`;
# only a slim image does not.
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl git jq ripgrep tini bubblewrap procps \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /work \
    && chown node:node /work

COPY --from=uze-build --chown=node:node /out/uze /usr/local/bin/uze

# UZE is its own provisioner: `uze setup` installs the primary V0 harnesses
# (Claude Code, Codex, OpenCode, Antigravity CLI) through each vendor's
# official installer — the same mechanism the product uses for its users.
# The image build runs it with network, so it installs the channel-latest
# versions; the L2 runtime then runs `--network none` and keeps them.
# `HOME=/usr/local` pins the vendor default install location (~/.local/bin)
# into the image, reachable by the runtime user; the PATH below carries it.
ENV PATH="/usr/local/.local/bin:/usr/local/.opencode/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
#
# The harness trees are handed to `node` in the same layer that installs
# them: a `chown -R` in a later layer copies every file it touches into that
# layer, which kept a second ~0.7 GB of the same binaries in the image and in
# the build cache of every rebuild.
RUN HOME=/usr/local /usr/local/bin/uze setup \
    && chown -R node:node /usr/local/.local /usr/local/.opencode

COPY --from=uze-build --chown=node:node /out/uze-mcp-conformance-fixture /usr/local/bin/uze-mcp-conformance-fixture
# The Lab owns a complete, isolated marketplace fixture. It represents the
# final resources exercised by real harnesses; unit/integration fixtures stay
# under tests/_fixtures and remain independent.
COPY --chown=node:node conformance/_fixtures /opt/uze-conformance-fixtures
COPY --chmod=0755 conformance/entrypoint.sh /usr/local/bin/uze-harness-entrypoint

# Rootless runtime: `node` (UID 1000) owns every writable tree — HOME,
# UZE_HOME, fixture cache, package-manager home, harness installs (vendors
# self-update in place; networkless L2 makes those writes no-ops, and a
# permission error must never be the reason a probe fails). No step of the
# runtime user path can escalate: read-only root, cap_drop ALL,
# no-new-privileges, tmpfs-only writes, no Docker socket, no host HOME.
USER node
WORKDIR /work
ENV HOME=/home/node
ENV UZE_HOME=/home/node/.uze
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/uze-harness-entrypoint"]
CMD ["sh"]
