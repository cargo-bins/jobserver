use std::{
    borrow::Cow,
    io,
    sync::{Arc, Condvar, Mutex},
    thread::{Builder, JoinHandle},
};

use crate::Command;

#[derive(Debug)]
pub struct Client {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    count: Mutex<usize>,
    cvar: Condvar,
}

#[derive(Debug)]
pub struct Acquired(());

impl Client {
    pub fn new(limit: usize) -> io::Result<Client> {
        Ok(Client {
            inner: Arc::new(Inner {
                count: Mutex::new(limit),
                cvar: Condvar::new(),
            }),
        })
    }

    pub unsafe fn open(_s: &str) -> Option<Client> {
        None
    }

    pub fn acquire(&self) -> io::Result<Acquired> {
        let mut lock = self.inner.count.lock().unwrap_or_else(|e| e.into_inner());
        while *lock == 0 {
            lock = self
                .inner
                .cvar
                .wait(lock)
                .unwrap_or_else(|e| e.into_inner());
        }
        *lock -= 1;
        Ok(Acquired(()))
    }

    pub fn release(&self, _data: Option<&Acquired>) -> io::Result<()> {
        let mut lock = self.inner.count.lock().unwrap_or_else(|e| e.into_inner());
        *lock += 1;
        drop(lock);
        self.inner.cvar.notify_one();
        Ok(())
    }

    pub fn string_arg(&self) -> Cow<'_, str> {
        panic!(
            "On this platform there is no cross process jobserver support,
             so Client::configure_and_run is not supported."
        );
    }

    pub fn pre_run<Cmd>(&self, cmd: &mut Cmd)
    where
        Cmd: Command,
    {
        panic!(
            "On this platform there is no cross process jobserver support,
             so Client::configure_and_run is not supported."
        );
    }
}

#[derive(Debug)]
pub struct Helper {
    thread: JoinHandle<()>,
}

pub(crate) fn spawn_helper(
    client: crate::Client,
    state: Arc<super::HelperState>,
    mut f: Box<dyn FnMut(io::Result<crate::Acquired>) + Send>,
) -> io::Result<Helper> {
    let thread = Builder::new().spawn(move || {
        state.for_each_request(|_| f(client.acquire()));
    })?;

    Ok(Helper { thread })
}

impl Helper {
    pub fn join(self) {
        // TODO: this is not correct if the thread is blocked in
        // `client.acquire()`.
        drop(self.thread.join());
    }
}
