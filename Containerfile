# Container image for the Ignit CLI.
#
# The image contains the `ignit` binary and the Rust toolchain plus system
# libraries needed to build generated desktop projects. Build it from the
# repository root:
#
#   docker build -f Containerfile -t ignit-cli .
#   podman build -t ignit-cli .
#
# Then mount a project or a parent directory into /workspace:
#
#   docker run --rm -it -v "$PWD:/workspace" ignit-cli init my-game
#   docker run --rm -it -v "$PWD:/workspace" ignit-cli build

# Keep the Rust image tags in sync with rust-toolchain.toml.
FROM docker.io/library/rust:1.100-bookworm AS builder

WORKDIR /src

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY Crates ./Crates
COPY Examples ./Examples

RUN cargo install --path Crates/cli --locked --root /opt/ignit

FROM docker.io/library/rust:1.100-slim-bookworm

# Keep the full Rust toolchain in the runtime image: `ignit build` invokes
# Cargo in the target project. The remaining packages are the Linux build and
# runtime dependencies for Ignit's windowing and graphics stack.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      build-essential \
      pkg-config \
      git \
      unzip \
      ca-certificates \
      libudev-dev \
      libwayland-dev \
      libxkbcommon-dev \
      libxkbcommon-x11-0 \
      libvulkan-dev \
      libx11-6 \
      libxcursor1 \
      libxi6 \
      libxrandr2 \
      mesa-vulkan-drivers \
      libgl1-mesa-dri \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /opt/ignit/bin/ignit /usr/local/bin/ignit

WORKDIR /workspace

ENTRYPOINT ["ignit"]
CMD ["--help"]
