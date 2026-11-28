# OSS-Fuzz's builder for Rust: clang, libFuzzer, cargo-fuzz and the sanitiser flags. Pinned by
# digest, which Renovate moves with the rest of the week's updates; the image is rebuilt daily.
FROM gcr.io/oss-fuzz-base/base-builder-rust:latest@sha256:53035c6f510f5f949828ab2cb70f551c3c37e43d94c26579228c9f1fa5fc6a1e
COPY . $SRC/steamgauge
WORKDIR $SRC/steamgauge
COPY .clusterfuzzlite/build.sh $SRC/
