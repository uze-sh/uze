//! The pool extension jobs run on (`extension-platform` D28): a fixed
//! number of threads for the whole client, and one lane per extension, so
//! an extension's jobs run one at a time and in the order asked, and a
//! burst of questions never takes more of the machine than the pool has.
//!
//! A lane waiting behind a busy one of its own extension is skipped, never
//! waited on: another extension's lane takes the free thread meanwhile.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

/// The threads every extension lane shares: the workspace's `jobs = 2`.
pub(super) const POOL_THREADS: usize = 2;

/// One job of a lane, run on whichever pool thread takes it.
pub(super) type Task = Box<dyn FnOnce() + Send>;

/// Lanes of tasks, run on a fixed set of threads.
pub(super) struct LanePool<L> {
    shared: Arc<Shared<L>>,
}

struct Shared<L> {
    queue: Mutex<Queue<L>>,
    wake: Condvar,
}

struct Queue<L> {
    /// What each lane has left to run, in the order it was asked.
    waiting: BTreeMap<L, VecDeque<Task>>,
    /// Lanes with a task to run and none running, in the order they
    /// became ready.
    ready: VecDeque<L>,
    running: BTreeSet<L>,
}

impl<L: Ord + Clone + Send + 'static> LanePool<L> {
    pub(super) fn new(threads: usize) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue {
                waiting: BTreeMap::new(),
                ready: VecDeque::new(),
                running: BTreeSet::new(),
            }),
            wake: Condvar::new(),
        });
        for index in 0..threads {
            let shared = Arc::clone(&shared);
            let _ = thread::Builder::new()
                .name(format!("uze-extension-{index}"))
                .spawn(move || work(&shared));
        }
        Self { shared }
    }

    /// Runs `task` after every task `lane` was handed before it.
    pub(super) fn submit(&self, lane: L, task: Task) {
        let mut queue = lock(&self.shared.queue);
        queue
            .waiting
            .entry(lane.clone())
            .or_default()
            .push_back(task);
        if !queue.running.contains(&lane) && !queue.ready.contains(&lane) {
            queue.ready.push_back(lane);
            self.shared.wake.notify_one();
        }
    }
}

/// One pool thread: the next ready lane's next task, for as long as the
/// client runs. A task that panics takes nothing else down with it.
fn work<L: Ord + Clone>(shared: &Shared<L>) {
    loop {
        let (lane, task) = {
            let mut queue = lock(&shared.queue);
            loop {
                if let Some(lane) = queue.ready.pop_front() {
                    let task = queue.waiting.get_mut(&lane).and_then(VecDeque::pop_front);
                    if let Some(task) = task {
                        queue.running.insert(lane.clone());
                        break (lane, task);
                    }
                    queue.waiting.remove(&lane);
                    continue;
                }
                queue = shared
                    .wake
                    .wait(queue)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
        let mut queue = lock(&shared.queue);
        queue.running.remove(&lane);
        if queue
            .waiting
            .get(&lane)
            .is_some_and(|tasks| !tasks.is_empty())
        {
            queue.ready.push_back(lane);
            shared.wake.notify_one();
        } else {
            queue.waiting.remove(&lane);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    /// A task that counts itself running while it sleeps, and says when
    /// it is done.
    fn counted(
        running: &Arc<AtomicUsize>,
        most: &Arc<AtomicUsize>,
        done: &mpsc::Sender<usize>,
        tag: usize,
    ) -> Task {
        let (running, most, done) = (Arc::clone(running), Arc::clone(most), done.clone());
        Box::new(move || {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(30));
            running.fetch_sub(1, Ordering::SeqCst);
            let _ = done.send(tag);
        })
    }

    fn finished(done: &mpsc::Receiver<usize>, count: usize) -> Vec<usize> {
        (0..count)
            .map(|_| {
                done.recv_timeout(Duration::from_secs(10))
                    .expect("the task ran")
            })
            .collect()
    }

    #[test]
    fn one_lane_runs_its_tasks_one_at_a_time_in_order() {
        let pool = LanePool::new(POOL_THREADS);
        let (running, most) = (Arc::default(), Arc::default());
        let (done, finishes) = mpsc::channel();
        for tag in 0..4 {
            pool.submit("board", counted(&running, &most, &done, tag));
        }
        assert_eq!(finished(&finishes, 4), [0, 1, 2, 3]);
        assert_eq!(most.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn lanes_share_the_pool_and_never_take_more_than_it_has() {
        let pool = LanePool::new(POOL_THREADS);
        let (running, most) = (Arc::default(), Arc::default());
        let (done, finishes) = mpsc::channel();
        for tag in 0..6 {
            pool.submit(tag % 3, counted(&running, &most, &done, tag));
        }
        let mut order = finished(&finishes, 6);
        assert_eq!(most.load(Ordering::SeqCst), POOL_THREADS);
        order.sort_unstable();
        assert_eq!(order, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_task_that_panics_leaves_its_lane_running() {
        let pool = LanePool::new(1);
        let (done, finishes) = mpsc::channel();
        pool.submit("board", Box::new(|| panic!("a script host bug")));
        let after = done.clone();
        pool.submit(
            "board",
            Box::new(move || {
                let _ = after.send(1);
            }),
        );
        assert_eq!(finished(&finishes, 1), [1]);
    }
}
