# Atlas Carries Us: one pinned definition of the tools that build, test and
# run it.
#
# Stages:
#   toolchain  Go, Node, Rust (stable, and the pinned nightly that the Rust
#              adapter runs) and Playwright's Chromium. The VS Code
#              devcontainer and CI use it; scripts/check.sh runs every check
#              inside it.
#   build      Builds the viewer, acu and the Go, TypeScript and Rust
#              adapters into /out.
#   export     The contents of /out alone, for `--output`: it writes the
#              Linux programs to a folder on the host. scripts/docker-export.sh
#              runs it; its --platform option picks another architecture,
#              such as linux/arm64, under emulation.
#   rust-nightly  The pinned nightly alone, for the runtime image.
#   runtime    acu and the three adapters, plus the Go toolchain that the Go
#              adapter needs to load packages, the Node runtime that runs the
#              TypeScript adapter, and the nightly Rust toolchain whose
#              rustdoc and rust-analyzer the Rust adapter runs. The default
#              target.
#
#   docker build --target toolchain -t acu-toolchain .
#   docker run --rm -v "$PWD:/workspace" acu-toolchain scripts/check.sh
#
#   docker build -t acu .
#   docker run --rm -v "$PWD:/work" -p 4321:4321 acu serve --addr 0.0.0.0:4321
#
#   scripts/docker-export.sh [--platform linux/arm64] [<folder>]
#   docker build --target export --output type=local,dest=out/docker .
#
# Each base image names its version in the tag and its exact content in the
# digest. To move to a new version, change both; the image's page on Docker
# Hub lists the digest of each tag.
#
# REGISTRY selects a mirror of Docker Hub, for example mirror.gcr.io, when
# Docker Hub limits anonymous pulls. The digests stay the same.
#
# Builds behind a TLS-inspecting proxy: pass the proxy's CA certificate as a
# build secret, and the proxy itself as build arguments:
#   docker build --secret id=extra_ca,src=/path/to/ca.pem \
#     --build-arg HTTPS_PROXY --build-arg https_proxy .

ARG REGISTRY=docker.io
ARG GO_IMAGE=${REGISTRY}/library/golang:1.26.11-bookworm@sha256:dc40a13b61304647ed7e5d581d246b86413dfc29d63c6d018310feb93cd98fc0
ARG NODE_IMAGE=${REGISTRY}/library/node:26.13.1-bookworm-slim@sha256:5df79989ed685e09f0a98019db4cebfb22fb9df1a53c0161664a011f7ff6b0f1
ARG RUST_IMAGE=${REGISTRY}/library/rust:1.97.1-slim-bookworm@sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3
# The nightly whose rustdoc writes the JSON format that the Rust adapter
# reads. Keep it in step with NIGHTLY in adapters/rust/src/toolchain.rs.
ARG RUST_NIGHTLY=nightly-2026-11-01

FROM ${GO_IMAGE} AS go
FROM ${NODE_IMAGE} AS node

# ---------------------------------------------------------------------------
FROM ${RUST_IMAGE} AS toolchain

# Keep in step with "playwright" in tests/e2e/package.json.
ARG PLAYWRIGHT_VERSION=1.63.0
ARG RUST_NIGHTLY

ENV GOTOOLCHAIN=local \
    GOPATH=/go \
    PATH=/usr/local/go/bin:/go/bin:${PATH} \
    PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers \
    NODE_USE_SYSTEM_CA=1

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# An optional extra CA certificate, then git, with apt over HTTPS.
RUN --mount=type=secret,id=extra_ca,required=false \
    if [ -s /run/secrets/extra_ca ]; then \
      cp /run/secrets/extra_ca /usr/local/share/ca-certificates/extra-ca.crt && update-ca-certificates; \
    fi \
 && sed -i 's#http://deb.debian.org#https://deb.debian.org#g' /etc/apt/sources.list.d/debian.sources \
 && apt-get update \
 && apt-get install -y --no-install-recommends git \
 && rm -rf /var/lib/apt/lists/*

COPY --from=go /usr/local/go /usr/local/go
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm

RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
 && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx \
 && rustup component add rustfmt clippy \
 && rustup toolchain install "${RUST_NIGHTLY}" --profile minimal --component rust-src,rust-analyzer \
 && npx --yes "playwright@${PLAYWRIGHT_VERSION}" install --with-deps chromium \
 && rm -rf /root/.npm /var/lib/apt/lists/* \
 && useradd --create-home --uid 1000 --shell /bin/bash dev \
 && mkdir -p /go /workspace \
 && chown -R dev:dev /go /usr/local/cargo /usr/local/rustup /workspace

WORKDIR /workspace

# ---------------------------------------------------------------------------
FROM toolchain AS build

WORKDIR /src
COPY package.json package-lock.json ./
COPY viewer/package.json viewer/
COPY adapters/typescript/package.json adapters/typescript/
COPY tests/e2e/package.json tests/e2e/
RUN npm ci --no-audit --no-fund
# The TypeScript adapter's run-time dependencies alone, from the same lock
# file: the TypeScript compiler, svelte and svelte2tsx.
RUN mkdir /deps \
 && cp -r package.json package-lock.json viewer adapters tests /deps/ \
 && (cd /deps && npm ci --omit=dev --no-audit --no-fund -w adapters/typescript) \
 && rm -rf /deps/node_modules/@atlas-carries-us /deps/node_modules/.bin /root/.npm
COPY adapters/go/go.mod adapters/go/go.sum adapters/go/
RUN cd adapters/go && go mod download
COPY . .
RUN npm run build -w viewer \
 && cargo build --release --locked --workspace \
 && mkdir -p /out \
 && cp target/release/acu target/release/acu-adapter-rust /out/ \
 && (cd adapters/go && CGO_ENABLED=0 go build -trimpath -o /out/acu-adapter-go ./cmd/acu-adapter-go)
# The TypeScript adapter: its compiled source, and its run-time dependencies
# from /deps. It needs nothing else at run time.
RUN npm run build -w adapters/typescript \
 && mkdir -p /out/acu-adapter-typescript/dist \
 && cp adapters/typescript/package.json /out/acu-adapter-typescript/ \
 && cp -r adapters/typescript/dist/src /out/acu-adapter-typescript/dist/src \
 && cp -r /deps/node_modules /out/acu-adapter-typescript/node_modules \
 && chmod +x /out/acu-adapter-typescript/dist/src/main.js

# ---------------------------------------------------------------------------
# The build output alone, with no base image. Use it with --output to write
# acu, acu-adapter-go, acu-adapter-rust and acu-adapter-typescript/ to a
# folder on the host. They target Linux with glibc 2.34 or later, on the
# architecture of --platform. Keep runtime the last stage, so that
# a build without --target still makes the runtime image.
FROM scratch AS export
COPY --from=build /out /

# ---------------------------------------------------------------------------
# The pinned nightly alone: the runtime image copies this stage's rustup and
# cargo folders, which then hold no stable toolchain.
FROM toolchain AS rust-nightly
ARG RUST_NIGHTLY
RUN rustup default "${RUST_NIGHTLY}" \
 && for t in $(rustup toolchain list | awk '{print $1}' | grep -v "^${RUST_NIGHTLY}"); do rustup toolchain uninstall "$t"; done \
 && rm -rf /usr/local/rustup/downloads /usr/local/rustup/tmp /usr/local/cargo/registry

# ---------------------------------------------------------------------------
FROM ${GO_IMAGE} AS runtime

LABEL org.opencontainers.image.title="Atlas Carries Us" \
      org.opencontainers.image.description="acu and its Go, TypeScript and Rust adapters: turn a code base and its doc comments into an interactive map." \
      org.opencontainers.image.source="https://github.com/revslaughter/atlas-carries-us" \
      org.opencontainers.image.licenses="MIT"

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:${PATH}
COPY --from=rust-nightly /usr/local/rustup /usr/local/rustup
COPY --from=rust-nightly /usr/local/cargo /usr/local/cargo
COPY --from=build /out/acu /out/acu-adapter-go /out/acu-adapter-rust /usr/local/bin/
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=build /out/acu-adapter-typescript /usr/local/lib/acu-adapter-typescript
RUN ln -s /usr/local/lib/acu-adapter-typescript/dist/src/main.js /usr/local/bin/acu-adapter-typescript
# The image exists to read a repository that you mount into it, which
# belongs to your user, not the container's. Let git read it, so acu can name
# the commit in source links.
RUN git config --system --add safe.directory '*'
WORKDIR /work
EXPOSE 4321
ENTRYPOINT ["acu"]
CMD ["--help"]
