# A complete, reproducible build environment for Assembly Teacher.
#
# The image carries more than the Rust toolchain on purpose. This project's test
# suite checks its own disassembler against `objdump` and its own assembler
# against `nasm`, and several lessons compile real C and inspect the resulting
# ELF. Those tools are therefore part of the build environment, not optional
# extras: without them the differential tests silently skip and the suite loses
# most of its value.
#
#   docker build -t asmteacher-dev -f contrib/Dockerfile .
#   docker run --rm -v "$PWD":/work asmteacher-dev contrib/test.sh
#
# Node is the base image because installing a current Node onto a Rust image is
# more awkward than installing Rust onto a Node image.

FROM node:22-bookworm-slim

ARG RUST_VERSION=1.96.0

# build-essential  -> gcc, ld, make; lessons compile C to show what a compiler emits
# nasm             -> the oracle for the assembler's differential tests
# binutils         -> objdump/readelf/objcopy; the oracle for the decoder and for binfmt
# gdb              -> the debugging chapter's demonstrations
# file, xxd        -> used by lesson scripts and by hand when poking at binaries
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        nasm \
        binutils \
        gdb \
        file \
        xxd \
        curl \
        ca-certificates \
        pkg-config \
        git \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y \
          --default-toolchain "${RUST_VERSION}" \
          --profile minimal \
          --component clippy rustfmt \
    && chmod -R a+w "$RUSTUP_HOME" "$CARGO_HOME"

# ASLR makes `gdb` demonstrations and any test that compares addresses
# non-reproducible. Disabling it needs --privileged at run time, so we do not
# rely on it; lessons that care set `setarch -R` themselves.

WORKDIR /work

# Warm the dependency cache so an incremental `docker build` after a source-only
# change does not refetch the registry. Copying just the manifests keeps this
# layer valid until a dependency actually changes.
COPY Cargo.toml Cargo.lock* ./
COPY crates/asm-core/Cargo.toml crates/asm-core/
COPY crates/asm-emu/Cargo.toml  crates/asm-emu/
COPY crates/binfmt/Cargo.toml   crates/binfmt/
COPY crates/lesson/Cargo.toml   crates/lesson/
COPY crates/server/Cargo.toml   crates/server/
RUN mkdir -p crates/asm-core/src crates/asm-emu/src crates/binfmt/src \
             crates/lesson/src crates/server/src \
    && echo '' > crates/asm-core/src/lib.rs \
    && echo '' > crates/asm-emu/src/lib.rs \
    && echo '' > crates/binfmt/src/lib.rs \
    && echo '' > crates/lesson/src/lib.rs \
    && echo 'fn main() {}' > crates/server/src/main.rs \
    && cargo fetch --locked 2>/dev/null || cargo fetch \
    && rm -rf crates

COPY web/package.json web/package-lock.json* web/
RUN cd web && (npm ci 2>/dev/null || npm install)

CMD ["bash"]
