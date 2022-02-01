#![no_main]
use dtn7_plus::client::WsSendData;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // fuzzed code goes here
    serde_cbor::from_slice::<WsSendData>(&data);
});
