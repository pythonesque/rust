// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Timers based on timerfd_create(2)
//!
//! On OSes which support timerfd_create, we can use these much more accurate
//! timers over select() + a timeout (see timer_other.rs). This strategy still
//! employs a worker thread which does the waiting on the timer fds (to send
//! messages away).
//!
//! The worker thread in this implementation uses epoll(7) to block. It
//! maintains a working set of *all* native timers in the process, along with a
//! pipe file descriptor used to communicate that there is data available on the
//! incoming channel to the worker thread. Timers send requests to update their
//! timerfd settings to the worker thread (see the comment above 'oneshot' for
//! why).
//!
//! As with timer_other, timers just using sleep() do not use the timerfd at
//! all. They remove the timerfd from the worker thread and then invoke
//! nanosleep() to block the calling thread.
//!
//! As with timer_other, all units in this file are in units of millseconds.

use std::comm::Data;
use libc;
use std::ptr;
use std::os;
use std::rt::rtio;
use std::mem;

use io::file::FileDesc;
use io::IoResult;
use io::timer_helper;

pub struct Timer {
    fd: FileDesc,
    on_worker: bool,
}

#[allow(visible_private_types)]
pub enum Req {
    NewTimer(libc::c_int, Sender<()>, bool, imp::itimerspec),
    RemoveTimer(libc::c_int, Sender<()>),
    Shutdown,
}

fn helper(input: libc::c_int, messages: Receiver<Req>) {
    let efd = unsafe { imp::epoll_create(10) };
    let _fd1 = FileDesc::new(input, true);
    let _fd2 = FileDesc::new(efd, true);

    fn add(efd: libc::c_int, fd: libc::c_int) {
        let event = imp::epoll_event {
            events: imp::EPOLLIN as u32,
            data: fd as i64,
        };
        let ret = unsafe {
            imp::epoll_ctl(efd, imp::EPOLL_CTL_ADD, fd, &event)
        };
        assert_eq!(ret, 0);
    }
    fn del(efd: libc::c_int, fd: libc::c_int) {
        let event = imp::epoll_event { events: 0, data: 0 };
        let ret = unsafe {
            imp::epoll_ctl(efd, imp::EPOLL_CTL_DEL, fd, &event)
        };
        assert_eq!(ret, 0);
    }

    add(efd, input);
    let events: [imp::epoll_event, ..16] = unsafe { mem::init() };
    let mut list: Vec<(libc::c_int, Sender<()>, bool)> = vec![];
    'outer: loop {
        let n = match unsafe {
            imp::epoll_wait(efd, events.as_ptr(),
                            events.len() as libc::c_int, -1)
        } {
            0 => fail!("epoll_wait returned immediately!"),
            -1 if os::errno() == libc::EINTR as int => { continue }
            -1 => fail!("epoll wait failed: {}", os::last_os_error()),
            n => n
        };

        let mut incoming = false;
        for event in events.slice_to(n as uint).iter() {
            let fd = event.data as libc::c_int;
            if fd == input {
                let mut buf = [0, ..1];
                // drain the input file descriptor of its input
                let _ = FileDesc::new(fd, false).inner_read(buf).unwrap();
                incoming = true;
            } else {
                let mut bits = [0, ..8];
                // drain the timerfd of how many times its fired
                //
                // FIXME: should this perform a send() this number of
                //      times?
                let _ = FileDesc::new(fd, false).inner_read(bits).unwrap();
                let (remove, i) = {
                    match list.as_slice().bsearch(|&(f, _, _)| f.cmp(&fd)) {
                        Some(i) => {
                            let (_, ref c, oneshot) = *list.get(i);
                            (!c.try_send(()) || oneshot, i)
                        }
                        None => fail!("fd not active: {}", fd),
                    }
                };
                if remove {
                    drop(list.remove(i));
                    del(efd, fd);
                }
            }
        }

        while incoming {
            match messages.try_recv() {
                Data(NewTimer(fd, chan, one, timeval)) => {
                    // acknowledge we have the new channel, we will never send
                    // another message to the old channel
                    chan.send(());

                    // If we haven't previously seen the file descriptor, then
                    // we need to add it to the epoll set.
                    match list.as_slice().bsearch(|&(f, _, _)| f.cmp(&fd)) {
                        Some(i) => {
                            drop(mem::replace(list.get_mut(i), (fd, chan, one)));
                        }
                        None => {
                            match list.iter().position(|&(f, _, _)| f >= fd) {
                                Some(i) => list.insert(i, (fd, chan, one)),
                                None => list.push((fd, chan, one)),
                            }
                            add(efd, fd);
                        }
                    }

                    // Update the timerfd's time value now that we have control
                    // of the timerfd
                    let ret = unsafe {
                        imp::timerfd_settime(fd, 0, &timeval, ptr::null())
                    };
                    assert_eq!(ret, 0);
                }

                Data(RemoveTimer(fd, chan)) => {
                    match list.as_slice().bsearch(|&(f, _, _)| f.cmp(&fd)) {
                        Some(i) => {
                            drop(list.remove(i));
                            del(efd, fd);
                        }
                        None => {}
                    }
                    chan.send(());
                }

                Data(Shutdown) => {
                    assert!(list.len() == 0);
                    break 'outer;
                }

                _ => break,
            }
        }
    }
}

impl Timer {
    pub fn new() -> IoResult<Timer> {
        timer_helper::boot(helper);
        match unsafe { imp::timerfd_create(imp::CLOCK_MONOTONIC, 0) } {
            -1 => Err(super::last_error()),
            n => Ok(Timer { fd: FileDesc::new(n, true), on_worker: false, }),
        }
    }

    pub fn sleep(ms: u64) {
        let mut to_sleep = libc::timespec {
            tv_sec: (ms / 1000) as libc::time_t,
            tv_nsec: ((ms % 1000) * 1000000) as libc::c_long,
        };
        while unsafe { libc::nanosleep(&to_sleep, &mut to_sleep) } != 0 {
            if os::errno() as int != libc::EINTR as int {
                fail!("failed to sleep, but not because of EINTR?");
            }
        }
    }

    fn remove(&mut self) {
        if !self.on_worker { return }

        let (tx, rx) = channel();
        timer_helper::send(RemoveTimer(self.fd.fd(), tx));
        rx.recv();
        self.on_worker = false;
    }
}

impl rtio::RtioTimer for Timer {
    fn sleep(&mut self, msecs: u64) {
        self.remove();
        Timer::sleep(msecs);
    }

    // Periodic and oneshot channels are updated by updating the settings on the
    // corresopnding timerfd. The update is not performed on the thread calling
    // oneshot or period, but rather the helper epoll thread. The reason for
    // this is to avoid losing messages and avoid leaking messages across ports.
    //
    // By updating the timerfd on the helper thread, we're guaranteed that all
    // messages for a particular setting of the timer will be received by the
    // new channel/port pair rather than leaking old messages onto the new port
    // or leaking new messages onto the old port.
    //
    // We also wait for the remote thread to actually receive the new settings
    // before returning to guarantee the invariant that when oneshot() and
    // period() return that the old port will never receive any more messages.

    fn oneshot(&mut self, msecs: u64) -> Receiver<()> {
        let (tx, rx) = channel();

        let new_value = imp::itimerspec {
            it_interval: imp::timespec { tv_sec: 0, tv_nsec: 0 },
            it_value: imp::timespec {
                tv_sec: (msecs / 1000) as libc::time_t,
                tv_nsec: ((msecs % 1000) * 1000000) as libc::c_long,
            }
        };
        timer_helper::send(NewTimer(self.fd.fd(), tx, true, new_value));
        rx.recv();
        self.on_worker = true;

        return rx;
    }

    fn period(&mut self, msecs: u64) -> Receiver<()> {
        let (tx, rx) = channel();

        let spec = imp::timespec {
            tv_sec: (msecs / 1000) as libc::time_t,
            tv_nsec: ((msecs % 1000) * 1000000) as libc::c_long,
        };
        let new_value = imp::itimerspec { it_interval: spec, it_value: spec, };
        timer_helper::send(NewTimer(self.fd.fd(), tx, false, new_value));
        rx.recv();
        self.on_worker = true;

        return rx;
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // When the timerfd file descriptor is closed, it will be automatically
        // removed from the epoll set of the worker thread, but we want to make
        // sure that the associated channel is also removed from the worker's
        // hash map.
        self.remove();
    }
}

#[allow(dead_code)]
mod imp {
    use libc;

    pub static CLOCK_MONOTONIC: libc::c_int = 1;
    pub static EPOLL_CTL_ADD: libc::c_int = 1;
    pub static EPOLL_CTL_DEL: libc::c_int = 2;
    pub static EPOLL_CTL_MOD: libc::c_int = 3;
    pub static EPOLLIN: libc::c_int = 0x001;
    pub static EPOLLOUT: libc::c_int = 0x004;
    pub static EPOLLPRI: libc::c_int = 0x002;
    pub static EPOLLERR: libc::c_int = 0x008;
    pub static EPOLLRDHUP: libc::c_int = 0x2000;
    pub static EPOLLET: libc::c_int = 1 << 31;
    pub static EPOLLHUP: libc::c_int = 0x010;
    pub static EPOLLONESHOT: libc::c_int = 1 << 30;

    #[cfg(target_arch = "x86_64")]
    #[packed]
    pub struct epoll_event {
        pub events: u32,
        pub data: i64,
    }

    #[cfg(not(target_arch = "x86_64"))]
    pub struct epoll_event {
        pub events: u32,
        pub data: i64,
    }

    pub struct timespec {
        pub tv_sec: libc::time_t,
        pub tv_nsec: libc::c_long,
    }

    pub struct itimerspec {
        pub it_interval: timespec,
        pub it_value: timespec,
    }

    extern {
        pub fn timerfd_create(clockid: libc::c_int,
                              flags: libc::c_int) -> libc::c_int;
        pub fn timerfd_settime(fd: libc::c_int,
                               flags: libc::c_int,
                               new_value: *itimerspec,
                               old_value: *itimerspec) -> libc::c_int;
        pub fn timerfd_gettime(fd: libc::c_int,
                               curr_value: *itimerspec) -> libc::c_int;

        pub fn epoll_create(size: libc::c_int) -> libc::c_int;
        pub fn epoll_ctl(epfd: libc::c_int,
                         op: libc::c_int,
                         fd: libc::c_int,
                         event: *epoll_event) -> libc::c_int;
        pub fn epoll_wait(epfd: libc::c_int,
                          events: *epoll_event,
                          maxevents: libc::c_int,
                          timeout: libc::c_int) -> libc::c_int;
    }
}
