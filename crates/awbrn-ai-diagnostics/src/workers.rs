//! Execute work in parallel and commit results in index order.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

/// Keep at most one uncommitted result per worker.
///
/// A worker must receive an acknowledgement before it takes more work.
/// Returning `true` from `commit` stops the run. Errors also stop the run.
pub(crate) fn run_ordered<T: Send, E>(
    count: usize,
    jobs: usize,
    play: impl Fn(usize) -> T + Sync,
    mut commit: impl FnMut(usize, T) -> Result<bool, E>,
) -> Result<(), E> {
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1).min(count) {
            let sender = sender.clone();
            let (next, play) = (&next, &play);
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= count {
                        break;
                    }
                    let (acknowledge, acknowledged) = mpsc::channel();
                    if sender
                        .send((
                            index,
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| play(index))),
                            acknowledge,
                        ))
                        .is_err()
                        || acknowledged.recv().is_err()
                    {
                        break;
                    }
                }
            });
        }
        drop(sender);
        let mut pending = BTreeMap::new();
        let mut written = 0;
        let result = (|| {
            for (index, output, acknowledge) in &receiver {
                pending.insert(index, (output, acknowledge));
                while let Some((output, acknowledge)) = pending.remove(&written) {
                    let output = output.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                    let done = commit(written, output)?;
                    written += 1;
                    if done {
                        return Ok(());
                    }
                    let _ = acknowledge.send(());
                }
            }
            Ok(())
        })();
        // Release workers that wait for a result to be committed.
        drop(pending);
        drop(receiver);
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Condvar, Mutex};

    #[test]
    fn a_slow_first_result_bounds_uncommitted_work() {
        let started = (Mutex::new(0), Condvar::new());
        let mut committed = Vec::new();
        run_ordered(
            100,
            3,
            |index| {
                let (lock, ready) = &started;
                let mut count = lock.lock().unwrap();
                *count += 1;
                ready.notify_all();
                if index == 0 {
                    count = ready.wait_while(count, |count| *count < 3).unwrap();
                    assert_eq!(*count, 3);
                }
                index
            },
            |index, output| {
                assert_eq!(index, output);
                committed.push(index);
                Ok::<_, ()>(index == 4)
            },
        )
        .unwrap();
        assert_eq!(committed, [0, 1, 2, 3, 4]);
        assert!(*started.0.lock().unwrap() <= 8);
    }

    #[test]
    #[should_panic(expected = "worker failure")]
    fn a_worker_panic_releases_waiting_workers() {
        let _ = run_ordered(
            100,
            4,
            |index| {
                assert_ne!(index, 0, "worker failure");
                index
            },
            |_, _| Ok::<_, ()>(false),
        );
    }

    #[test]
    fn an_error_releases_waiting_workers() {
        let result = run_ordered(
            100,
            4,
            |index| index,
            |index, _| {
                if index == 2 { Err("stop") } else { Ok(false) }
            },
        );
        assert_eq!(result, Err("stop"));
    }
}
