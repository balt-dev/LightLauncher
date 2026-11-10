use async_compat::Compat;
use crossbeam_queue::SegQueue;
use futures::stream::FuturesUnordered;
use indexmap::IndexMap;
use reqwest::{
    Method, StatusCode, Url,
};
use smol::{Executor, Task, lock::OnceCell};
use std::{
    panic::catch_unwind,
    sync::{
        LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{
    models::{
        Account, AuthRequest, FetchStatus, RefreshRequest, RefreshResponse, ServerDisplay,
        ServerEntry, ServerInfo, ServerStatus,
    }, utils::{self, SerdeNever, request_bytes, request_json},
};

pub(crate) static EXECUTOR_THREADS: AtomicUsize = AtomicUsize::new(0);
static EXECUTOR: OnceCell<Executor> = OnceCell::new();

fn exec() -> &'static Executor<'static> {
    EXECUTOR.get_or_init_blocking(|| {
        let worker_count = EXECUTOR_THREADS.load(Ordering::Relaxed);
        if worker_count == 0 {
            panic!("tried to load executor before executor thread count was set");
        }
        for n in 1..=worker_count {
            std::thread::Builder::new()
                .name(format!("worker-{}", n))
                .spawn(|| {
                    loop {
                        catch_unwind(|| smol::block_on(exec().run(smol::future::pending::<()>())))
                            .ok();
                    }
                })
                .expect("could not spawn executor thread");
        }
        let ex = Executor::new();
        ex.spawn(smol::process::driver()).detach();
        ex
    })
}

pub enum FileDialogTarget {
    LoaderPath,
    ReplayPath
}

pub struct WorkerTask {
}

pub fn start(routine: WorkerTask) {
    
}
