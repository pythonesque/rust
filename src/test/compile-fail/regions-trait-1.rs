// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct ctxt { v: uint }

trait get_ctxt {
    // Here the `&` is bound in the method definition:
    fn get_ctxt(&self) -> &ctxt;
}

struct has_ctxt<'a> { c: &'a ctxt }

impl<'a> get_ctxt for has_ctxt<'a> {

    // Here an error occurs because we used `&self` but
    // the definition used `&`:
    fn get_ctxt(&self) -> &'a ctxt { //~ ERROR method `get_ctxt` has an incompatible type
        self.c
    }

}

fn get_v(gc: ~get_ctxt) -> uint {
    gc.get_ctxt().v
}

fn main() {
    let ctxt = ctxt { v: 22u };
    let hc = has_ctxt { c: &ctxt };
    assert_eq!(get_v(~hc as ~get_ctxt), 22u);
}
