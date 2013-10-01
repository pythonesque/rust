// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern mod rustpkg;
extern mod rustc;

use std::{io, os};
use rustpkg::api;
use rustpkg::version::NoVersion;

pub fn main() {
    let args = os::args();

// by convention, first arg is sysroot
    if args.len() < 2 {
        fail2!("Package script requires a directory where rustc libraries live as the first \
               argument");
    }

    let sysroot_arg = args[1].clone();
    let sysroot = Path(sysroot_arg);
    if !os::path_exists(&sysroot) {
        fail2!("Package script requires a sysroot that exists;{} doesn't", sysroot.to_str());
    }

    if args[2] != ~"install" {
        println!("Warning: I don't know how to {}", args[2]);
        return;
    }

    let out_path = os::self_exe_path().expect("Couldn't get self_exe path");

    let file = io::file_writer(&out_path.push("generated.rs"), [io::Create]).unwrap();
    file.write_str("pub fn wheeeee() { let xs = [1, 2, 3]; \
                   for _ in xs.iter() { assert!(true); } }");

    let context = api::default_context(sysroot, api::default_workspace());
    api::install_pkg(&context, os::getcwd(), ~"fancy-lib", NoVersion, ~[]);
}
