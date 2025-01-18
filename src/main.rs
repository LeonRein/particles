#![feature(sync_unsafe_cell, mpmc_channel, duration_millis_float)]
mod app_softbuffer;
mod scoped_threadpool;
// mod app_minifb;
mod particles;

fn main() {
    app_softbuffer::run();
    // app_minifb::run();
}
