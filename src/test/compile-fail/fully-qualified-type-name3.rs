// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test that we use fully-qualified type names in error messages.

// ignore-test

type T1 = uint;
type T2 = int;

fn bar(x: T1) -> T2 {
    return x;
    //~^ ERROR mismatched types: expected `T2` but found `T1`
}

fn main() {
}
