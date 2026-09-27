#![no_main]

// Lossy for the reason the splitter's target gives.
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    steamgauge_fuzz::stored_span(&String::from_utf8_lossy(data));
});
