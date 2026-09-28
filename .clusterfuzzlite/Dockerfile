# OSS-Fuzz's builder for Rust: clang, libFuzzer, cargo-fuzz and the sanitiser flags. Pinned by
# digest, which Renovate moves with the rest of the week's updates; the image is rebuilt daily.
FROM gcr.io/oss-fuzz-base/base-builder-rust:latest@sha256:5fc0de2bcb2eee5ab04db05ef175f215ab0fc30deb0c509403125cefd2fc8689
# ort's build script fetches ONNX Runtime over native TLS, which on Linux is OpenSSL, found
# through pkg-config. The CI runners carry both; this image carries neither.
RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev pkg-config
COPY . $SRC/steamgauge
WORKDIR $SRC/steamgauge
COPY .clusterfuzzlite/build.sh $SRC/
