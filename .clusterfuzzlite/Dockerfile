# OSS-Fuzz's builder for Rust: clang, libFuzzer, cargo-fuzz and the sanitiser flags. Pinned by
# digest, which Renovate moves with the rest of the week's updates; the image is rebuilt daily.
FROM gcr.io/oss-fuzz-base/base-builder-rust:latest@sha256:3428d0a90b1e7efbe2ba947bd1d0c9a70594986b4b6553804547f1fb24cfc92f
COPY . $SRC/steamgauge
WORKDIR $SRC/steamgauge
COPY .clusterfuzzlite/build.sh $SRC/
