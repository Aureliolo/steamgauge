# OSS-Fuzz's builder for Rust: clang, libFuzzer, cargo-fuzz and the sanitiser flags. Pinned by
# digest, which Renovate moves with the rest of the week's updates; the image is rebuilt daily.
FROM gcr.io/oss-fuzz-base/base-builder-rust:latest@sha256:412929965850a4c232f4191fece4f44d6726de53249698e95e5d4134117f38e4
# ort's build script fetches ONNX Runtime over native TLS, which on Linux is OpenSSL, found
# through pkg-config. The CI runners carry both; this image carries neither.
RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev pkg-config
COPY . $SRC/steamgauge
WORKDIR $SRC/steamgauge
COPY .clusterfuzzlite/build.sh $SRC/
