#![allow(dead_code)]

use std::marker::PhantomData;
use std::mem;
use std::sync::mpsc::{Receiver, RecvError, Sender, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

enum Message {
    NewJob(Thunk<'static>),
    Join,
}

trait FnBox {
    fn call_box(self: Box<Self>, id: usize);
}

impl<F: FnOnce(usize)> FnBox for F {
    fn call_box(self: Box<F>, id: usize) {
        (*self)(id)
    }
}

type Thunk<'a> = Box<dyn FnBox + Send + 'a>;

/// A threadpool that acts as a handle to a number
/// of threads spawned at construction.
pub struct Pool {
    job_sender: Sender<Message>,
    threads: Vec<ThreadData>,
}

struct ThreadData {
    _thread_join_handle: JoinHandle<()>,
    pool_sync_rx: Receiver<()>,
    thread_sync_tx: SyncSender<()>,
}

impl Pool {
    /// Construct a threadpool with the given number of threads.
    /// Minimum value is `1`.
    pub fn new(n: usize) -> Pool {
        assert!(n >= 1);

        let (job_sender, job_receiver) = channel();
        let job_receiver = Arc::new(Mutex::new(job_receiver));

        let mut threads = Vec::with_capacity(n as usize);

        // spawn n threads, put them in waiting mode
        for id in 0..n {
            let job_receiver = job_receiver.clone();

            let (pool_sync_tx, pool_sync_rx) = sync_channel::<()>(0);
            let (thread_sync_tx, thread_sync_rx) = sync_channel::<()>(0);

            let thread = thread::spawn(move || {
                loop {
                    let message = {
                        // Only lock jobs for the time it takes
                        // to get a job, not run it.
                        let lock = job_receiver.lock().unwrap();
                        lock.recv()
                    };

                    match message {
                        Ok(Message::NewJob(job)) => {
                            job.call_box(id);
                        }
                        Ok(Message::Join) => {
                            // Syncronize/Join with pool.
                            // This has to be a two step
                            // process to ensure that all threads
                            // finished their work before the pool
                            // can continue

                            // Wait until the pool started syncing with threads
                            if pool_sync_tx.send(()).is_err() {
                                // The pool was dropped.
                                break;
                            }

                            // Wait until the pool finished syncing with threads
                            if thread_sync_rx.recv().is_err() {
                                // The pool was dropped.
                                break;
                            }
                        }
                        Err(..) => {
                            // The pool was dropped.
                            break;
                        }
                    }
                }
            });

            threads.push(ThreadData {
                _thread_join_handle: thread,
                pool_sync_rx,
                thread_sync_tx,
            });
        }

        Pool {
            threads,
            job_sender,
        }
    }

    /// Borrows the pool and allows executing jobs on other
    /// threads during that scope via the argument of the closure.
    ///
    /// This method will block until the closure and all its jobs have
    /// run to completion.
    pub fn scoped<'pool, 'scope, F, R>(&'pool self, f: F) -> R
    where
        F: FnOnce(&Scope<'pool, 'scope>) -> R,
    {
        let scope = Scope {
            pool: self,
            _marker: PhantomData,
        };
        f(&scope)
    }

    /// Returns the number of threads inside this pool.
    pub fn thread_count(&self) -> u32 {
        self.threads.len() as u32
    }
}

/////////////////////////////////////////////////////////////////////////////

/// Handle to the scope during which the threadpool is borrowed.
pub struct Scope<'pool, 'scope> {
    pool: &'pool Pool,
    // The 'scope needs to be invariant... it seems?
    _marker: PhantomData<::std::cell::Cell<&'scope mut ()>>,
}

impl<'scope> Scope<'_, 'scope> {
    /// Execute a job on the threadpool.
    ///
    /// The body of the closure will be send to one of the
    /// internal threads, and this method itself will not wait
    /// for its completion.
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce(usize) + Send + 'scope,
    {
        let b = unsafe { mem::transmute::<Thunk<'scope>, Thunk<'static>>(Box::new(f)) };
        self.pool.job_sender.send(Message::NewJob(b)).unwrap();
    }

    /// Blocks until all currently queued jobs have run to completion.
    pub fn join_all(&self) {
        for _ in 0..self.pool.threads.len() {
            self.pool.job_sender.send(Message::Join).unwrap();
        }

        // Synchronize/Join with threads
        // This has to be a two step process
        // to make sure _all_ threads received _one_ Join message each.

        // This loop will block on every thread until it
        // received and reacted to its Join message.
        let mut worker_panic = false;
        for thread_data in &self.pool.threads {
            if let Err(RecvError) = thread_data.pool_sync_rx.recv() {
                worker_panic = true;
            }
        }
        if worker_panic {
            // Now that all the threads are paused, we can safely panic
            panic!("Thread pool worker panicked");
        }

        // Once all threads joined the jobs, send them a continue message
        for thread_data in &self.pool.threads {
            thread_data.thread_sync_tx.send(()).unwrap();
        }
    }
}

impl Drop for Scope<'_, '_> {
    fn drop(&mut self) {
        self.join_all();
    }
}

/////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {

    use super::Pool;
    use std::sync;
    use std::thread;
    use std::time;

    fn sleep_ms(ms: u64) {
        thread::sleep(time::Duration::from_millis(ms));
    }

    #[test]
    fn smoketest() {
        let pool = Pool::new(4);

        for i in 1..7 {
            let mut vec = vec![0, 1, 2, 3, 4];
            pool.scoped(|s| {
                for e in vec.iter_mut() {
                    s.execute(move |_| {
                        *e += i;
                    });
                }
            });

            let mut vec2 = vec![0, 1, 2, 3, 4];
            for e in vec2.iter_mut() {
                *e += i;
            }

            assert_eq!(vec, vec2);
        }
    }

    #[test]
    #[should_panic]
    fn thread_panic() {
        let pool = Pool::new(4);
        pool.scoped(|scoped| {
            scoped.execute(move |_| panic!());
        });
    }

    #[test]
    #[should_panic]
    fn scope_panic() {
        let pool = Pool::new(4);
        pool.scoped(|_scoped| panic!());
    }

    #[test]
    #[should_panic]
    fn pool_panic() {
        let _pool = Pool::new(4);
        panic!()
    }

    #[test]
    fn join_all() {
        let pool = Pool::new(4);

        let (tx_, rx) = sync::mpsc::channel();

        pool.scoped(|scoped| {
            let tx = tx_.clone();
            scoped.execute(move |_| {
                sleep_ms(1000);
                tx.send(2).unwrap();
            });

            let tx = tx_.clone();
            scoped.execute(move |_| {
                tx.send(1).unwrap();
            });

            scoped.join_all();

            let tx = tx_.clone();
            scoped.execute(move |_| {
                tx.send(3).unwrap();
            });
        });

        assert_eq!(rx.iter().take(3).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn join_all_with_thread_panic() {
        use std::sync::mpsc::Sender;
        struct OnScopeEnd(Sender<u8>);
        impl Drop for OnScopeEnd {
            fn drop(&mut self) {
                self.0.send(1).unwrap();
                sleep_ms(200);
            }
        }
        let (tx_, rx) = sync::mpsc::channel();
        // Use a thread here to handle the expected panic from the pool. Should
        // be switched to use panic::recover instead when it becomes stable.
        let handle = thread::spawn(move || {
            let pool = Pool::new(8);
            let _on_scope_end = OnScopeEnd(tx_.clone());
            pool.scoped(|scoped| {
                scoped.execute(move |_| {
                    sleep_ms(100);
                    panic!();
                });
                for _ in 1..8 {
                    let tx = tx_.clone();
                    scoped.execute(move |_| {
                        sleep_ms(200);
                        tx.send(0).unwrap();
                    });
                }
            });
        });
        if handle.join().is_ok() {
            panic!("Pool didn't panic as expected");
        }
        // If the `1` that OnScopeEnd sent occurs anywhere else than at the
        // end, that means that a worker thread was still running even
        // after the `scoped` call finished, which is unsound.
        let values: Vec<u8> = rx.into_iter().collect();
        assert_eq!(&values[..], &[0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn safe_execute() {
        let pool = Pool::new(4);
        pool.scoped(|scoped| {
            scoped.execute(move |_| {});
        });
    }
}
