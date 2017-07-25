#![feature(test)]
#![no_main]
#[macro_use] extern crate libfuzzer_sys;
#[macro_use] extern crate exonum;
extern crate test;

use std::sync::Arc;
use exonum::messages::MessageBuffer;
use exonum::messages::FromRaw;

message!{
    struct Test {
        const TYPE = 0;
        const ID = 1;
        const SIZE = 32;
        field first: u64 [0 => 8]
        field second: Vec<u64> [8 => 16]
        field third: &str [16 => 24]
        field fourth: u64 [24 => 32]
    }
}
fuzz_target!(|data: Vec<u8> | {
    if data.len() < ::exonum::messages::HEADER_LENGTH 
                    + ::exonum::crypto::SIGNATURE_LENGTH {
        return
    }
    let buff = Arc::new(MessageBuffer::from_vec(data));
    if let Ok(test) = Test::from_raw(buff) {
    let _ = ::test::black_box((test.first(),
        test.second(),
        test.third(),
        test.fourth()));
    }
});
