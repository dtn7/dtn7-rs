#![no_main]
use dtn7::cla::mtcp;
use libfuzzer_sys::fuzz_target;
use tokio_util::codec::Decoder;

fuzz_target!(|data: &[u8]| {
    // fuzzed code goes here
    let mut mpdu: mtcp::MPDUCodec = mtcp::MPDUCodec::new();

    mpdu.decode(&mut bytes::BytesMut::from(data));
});
