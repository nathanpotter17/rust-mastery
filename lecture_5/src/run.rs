use crate::manifest::Task;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

// A hasher of our own

// FNV-1a, a tiny hash function. Implementing std's Hasher trait lets every Hash type feed itself in: strings, numbers, tuples, Vecs.
pub struct Fnv1a(u64);

impl Default for Fnv1a {
    fn default() -> Fnv1a {
        Fnv1a(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for Fnv1a {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

// An artifact's hash covers its name and its dependencies' hashes, like a build cache's key: change anything upstream, and every hash downstream changes.
fn artifact_hash(task: &Task, done: &HashMap<String, u64>) -> u64 {
    let mut hasher = Fnv1a::default();
    task.name.hash(&mut hasher);
    for dependency in &task.dependencies {
        done[dependency].hash(&mut hasher);
    }
    hasher.finish()
}

// Running on threads

pub struct Run {
    pub hashes: HashMap<String, u64>,
    pub busy: Duration,
    pub elapsed: Duration,
}

// Runs each wave's tasks at the same time, one thread per task. Running a task means sleeping for its cost, then hashing.
pub fn run(waves: &[Vec<&Task>]) -> Run {
    let started = Instant::now();
    let mut hashes = HashMap::new();

    // The total time spent in tasks, added to by every thread. A Mutex lets one thread at a time change what's inside.
    let busy = Mutex::new(Duration::ZERO);

    for wave in waves {
        // A channel: any number of Senders, one Receiver. Each thread sends its result back through it.
        let (sender, receiver) = mpsc::channel();

        // Every thread spawned in a scope is joined before scope() returns, so the threads may borrow our locals: hashes, busy and the tasks.
        thread::scope(|scope| {
            for &task in wave {
                let sender = sender.clone();
                let hashes = &hashes;
                let busy = &busy;

                // move: the closure takes its own sender, and copies of the references.
                scope.spawn(move || {
                    thread::sleep(task.cost);

                    // lock() waits for the Mutex and returns a guard; the Mutex unlocks when the guard is dropped, at the end of this statement. It fails only if a thread panicked while holding it.
                    *busy.lock().expect("no thread panics while holding busy") += task.cost;

                    // send fails only once the Receiver is gone, and ours outlives every thread.
                    sender.send((task.name.clone(), artifact_hash(task, hashes))).expect("the receiver is alive");
                });
            }
        });

        // Our own Sender must go too: the Receiver's iterator ends once every Sender is dropped.
        drop(sender);
        hashes.extend(receiver);
    }

    // into_inner() ends the Mutex and hands back what it held.
    let busy = busy.into_inner().expect("no thread panicked");
    Run { hashes, busy, elapsed: started.elapsed() }
}
