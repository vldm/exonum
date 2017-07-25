#![no_main]
#![feature(test)]
#[macro_use] extern crate libfuzzer_sys;
#[macro_use] extern crate exonum;
extern crate test;

use exonum::encoding::Field;

encoding_struct!{
    struct Test {
        const SIZE = 32;
        field first: u64 [0 => 8]
        field second: Vec<u64> [8 => 16]
        field third: &str [16 => 24]
        field fourth: u64 [24 => 32]
    }
}
fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }
    if let Ok(_) = <Test as Field>::check(&data, 0.into(), 8.into(), 0.into()) {
        let test: Test = unsafe{ Field::read(&data, 0, 8)};
        let _ = ::test::black_box((test.first(),
            test.second(),
            test.third(),
            test.fourth()));
    }

});
