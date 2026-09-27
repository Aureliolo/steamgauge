# OSS-Fuzz's builder for Rust: clang, libFuzzer, cargo-fuzz and the sanitiser flags. Pinned by
# digest, which Renovate moves with the rest of the week's updates; the image is rebuilt daily.
FROM gcr.io/oss-fuzz-base/base-builder-rust:latest@sha256:ea76ce49f744afa314027b22953a0f8575694217bb9cbd5eccfb8319d15c54df
# ort's build script fetches ONNX Runtime over native TLS, which on Linux is OpenSSL, found
# through pkg-config. The CI runners carry both; this image carries neither.
RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev pkg-config
COPY . $SRC/steamgauge
WORKDIR $SRC/steamgauge
COPY .clusterfuzzlite/build.sh $SRC/
