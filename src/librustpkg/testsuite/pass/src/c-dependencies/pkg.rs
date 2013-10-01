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

use std::{io, os, task};
use rustpkg::api;
use rustpkg::version::NoVersion;
use rustpkg::workcache_support::digest_file_with_date;
use rustpkg::exit_codes::COPY_FAILED_CODE;

pub fn main() {
    let args = os::args();

// by convention, first arg is sysroot
    if args.len() < 2 {
        fail2!("Package script requires a directory where rustc libraries live as the first \
               argument");
    }

    let path_for_db = api::default_workspace();
    debug2!("path_for_db = {}", path_for_db.to_str());

    let sysroot_arg = args[1].clone();
    let sysroot = Path(sysroot_arg);
    if !os::path_exists(&sysroot) {
        fail2!("Package script requires a sysroot that exists; {} doesn't", sysroot.to_str());
    }

    if args[2] != ~"install" {
        io::println(format!("Warning: I don't know how to {}", args[2]));
        return;
    }

    let mut context = api::default_context(sysroot, path_for_db);
    let my_workspace = api::my_workspace(&context.context, "cdep");
    let foo_c_name = my_workspace.push_many(["src", "cdep-0.1", "foo.c"]);

    let out_lib_path = do context.workcache_context.with_prep("foo.c") |prep| {
        let sub_cx = context.context.clone();
        debug2!("foo_c_name = {}", foo_c_name.to_str());
        prep.declare_input("file", foo_c_name.to_str(), digest_file_with_date(&foo_c_name));
        let out_path = do prep.exec |exec| {
            let out_path = api::build_library_in_workspace(exec,
                                                           &mut sub_cx.clone(),
                                                           "cdep",
                                                           "gcc",
                                                           [~"-c"],
                                                           [~"foo.c"],
                                                           "foo");
            let out_p = Path(out_path);
            out_p.to_str()
        };
        out_path.to_str()
    };
    let out_lib_path = Path(out_lib_path);
    debug2!("out_lib_path = {}", out_lib_path.to_str());
    context.add_library_path(out_lib_path.dir_path());

    let context_clone = context.clone();
    let task_res = do task::try {
        let mut cc = context_clone.clone();
        api::install_pkg(&mut cc,
                         os::getcwd(),
                         ~"cdep",
                         NoVersion,
                         ~[(~"binary", out_lib_path.clone()), (~"file", foo_c_name.clone())]);
    };

    if task_res.is_err() {
        os::set_exit_status(COPY_FAILED_CODE);
    }
}
