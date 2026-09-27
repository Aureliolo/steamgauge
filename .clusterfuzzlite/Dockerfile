# OSS-Fuzz's builder for Rust: clang, libFuzzer, cargo-fuzz and the sanitiser flags. Pinned by
# digest, which Renovate moves with the rest of the week's updates; the image is rebuilt daily.
FROM gcr.io/oss-fuzz-base/base-builder-rust:latest@sha256:ea76ce49f744afa314027b22953a0f8575694217bb9cbd5eccfb8319d15c54df
COPY . $SRC/steamgauge
WORKDIR $SRC/steamgauge
COPY .clusterfuzzlite/build.sh $SRC/
