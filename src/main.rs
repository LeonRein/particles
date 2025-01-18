#![feature(sync_unsafe_cell, mpmc_channel)]
mod app_softbuffer;
mod scoped_threadpool;
// mod app_minifb;
mod particles;

fn main() {
    app_softbuffer::run();
    // app_minifb::run();
}
