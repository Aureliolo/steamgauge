#![no_main]

// Lossy rather than refused: a review arrives as UTF-8 through JSON, and every mutation the
// fuzzer makes is then a review worth splitting rather than a discarded run.
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    steamgauge_fuzz::splitter(&String::from_utf8_lossy(data));
});
