#!/bin/bash -eu
# Builds every fuzz target for the sanitiser ClusterFuzzLite asks for, and lays each one in $OUT
# with the seed corpus, the dictionary and the options it runs with.
set -o pipefail

# The compiler the product is built with, as rust-toolchain.toml names it and Renovate moves it,
# rather than the nightly the image carries: OSS-Fuzz pins that by hand, it lags the rust-version
# the crates declare, and cargo refuses to build them with it. RUSTC_BOOTSTRAP is what lets a
# stable compiler take the -Z flags a sanitiser build passes it.
toolchain="$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)"
rustup toolchain install "${toolchain}" --profile minimal
export RUSTUP_TOOLCHAIN="${toolchain}"
export RUSTC_BOOTSTRAP=1

# The sanitiser and coverage flags OSS-Fuzz puts in CFLAGS reach the C in every build script and
# proc macro as well, because here the host and the target are one triple, and the compiler
# cannot load a proc macro linked against AddressSanitizer: jieba compresses its dictionary at
# compile time with zstd, through one. The code under test is Rust, instrumented by the
# RUSTFLAGS cargo applies to the target alone, so the C it links is built plainly.
export CFLAGS="-O1 -fno-omit-frame-pointer -gline-tables-only"
export CXXFLAGS="${CFLAGS}"

# Debug assertions on, so that an overflow in the offset arithmetic panics here rather than
# wrapping silently as it would in the release binary.
cargo fuzz build --release --debug-assertions

for target in $(cargo fuzz list); do
  cp "fuzz/target/x86_64-unknown-linux-gnu/release/${target}" "${OUT}/"
  cp fuzz/review.dict "${OUT}/${target}.dict"
  cp fuzz/review.options "${OUT}/${target}.options"
  zip -q -j "${OUT}/${target}_seed_corpus.zip" fuzz/seeds/*
done
