# Lecture 5: More of the standard library

By the end of this lecture we'll have a tiny build planner, the core of tools like `make` and Cargo itself. It reads a manifest of tasks, each with a cost and the tasks it depends on, and works out an order that respects every dependency. Then it plans the build three ways: one task at a time, on two workers, and with every independent task at once, on real threads. It writes the schedule to a file, reads it back, and finally shows what it says about five broken manifests.

Lecture 4 used the standard library's vocabulary types. This lecture goes wider:
- **Errors with reasons:** `Result<T, E>`, our own error enums, `Display`, the `Error` trait and its `source()` chain, `From` and `?`, `Box<dyn Error>`, and `main` returning a `Result`.
- **Traits that hook into the language:** `FromStr` (what `parse` calls), `Display` (what `{}` prints), `Hasher` (what every `HashMap` uses), and `Ord` on tuples.
- **Collections:** `HashMap` and its `entry` API, `HashSet`, `BTreeMap` and `BTreeSet`'s sorted order, `VecDeque` as a queue, and `BinaryHeap` with `Reverse` as a priority queue.
- **Text, files and the system:** `split_once`, `split_whitespace`, `lines`, raw strings; `File`, `BufReader`, `BufWriter`, `writeln!`; `std::env::args` and `temp_dir`.
- **Time and threads:** `Duration` and `Instant`; `thread::scope`, channels (`mpsc`), and `Mutex`.

## 5.1 Project layout and the manifest

### Why
Parsing, planning and running are three concerns, so each gets a module. The manifest format comes first, since everything else follows from it.

### How
- **The modules:**
  - `src/manifest.rs`: tasks, manifests, parsing, and the error types.
  - `src/plan.rs`: the build order, waves, final targets and the two-worker schedule.
  - `src/run.rs`: running the waves on threads, and a hasher of our own.
  - `src/main.rs`: the built-in manifest, files, error reporting, and `main`.
- **The manifest format:** one task per line, `name cost: dependencies`, with the cost in milliseconds. `#` starts a comment, and blank lines are skipped:
  ```text
  engine   30: math gpu
  ```
  says the task `engine` takes 30 ms, and may start only after `math` and `gpu` have finished.

### Code
From the repo root:
```bash
cargo new lecture_5
```

`lecture_5/Cargo.toml`:
```toml
[package]
name = "lecture_5"
version = "0.1.0"
edition = "2024"
```

## 5.2 Errors and parsing: `src/manifest.rs`

### Why
Lecture 4's parser returned `Option`: it could say "no", but not *why*. A manifest can be wrong in many ways, and a person has to fix it, so every failure needs a reason, and the reason needs to say where. We also want `"app 40: engine ui".parse::<Task>()` to work like parsing a number, and `{}` to print our types.

### How
- **`Result<T, E>`** is the standard enum for an operation that can fail: `Ok(value)` or `Err(error)`. It's `Option` with a reason attached, and works much the same way: `map`, `and_then` and `?` all apply, and `ok()` turns it into an `Option`.
- **An error enum per level:**
  - **`TaskError`** lists everything that can be wrong with one line: a missing colon, name or cost, an extra field, or a cost that isn't a number. `BadCost` *carries* the `ParseIntError` that `parse` returned, and `ExtraField` the unexpected text.
  - **`ManifestError`** lists what can be wrong with a whole manifest. `BadLine` wraps a `TaskError` together with the line number it came from: the `TaskError` knows *what*, and the `ManifestError` adds *where*.
- **`Display`** is the trait behind `{}`. Its one method, `fmt`, writes the value into a `Formatter` with `write!`, which works like `println!` but returns a `fmt::Result`. `Display` is for people; `Debug` (`{:?}`) is for programmers, and we derive it.
  - **`?` inside `fmt`:** `Task`'s `fmt` writes in two steps, and `write!(...)?` stops early if the first fails.
  - **A padded `Duration`:** `Duration`'s `Debug` prints a unit, like `40ms`, and accepts a width like any other value: `{:>5?}` right-aligns it in 5 columns.
- **The `Error` trait** marks a type as an error. It requires `Debug` and `Display`, and offers `source()`: the error that caused this one, if any. `TaskError::BadCost` returns the `ParseIntError` inside it, and `ManifestError::BadLine` returns its `TaskError`, so a caller can walk the whole chain. `&(dyn Error + 'static)` is "a reference to any error type that holds no short-lived references".
- **`From` and `?`:** when `?` meets an `Err`, it converts the error with `From` into the function's own error type before returning it. `impl From<ParseIntError> for TaskError` is what lets `cost.parse()?` return a `TaskError`.
- **`FromStr`** is the trait `str::parse` calls. Implementing it for `Task`, with `type Err = TaskError`, makes `line.parse::<Task>()` work, and so does `let task: Task = line.parse()?`.
- **Parsing a line:**
  - `split_once(':')` returns `Some((before, after))` around the first `:`, or `None`. `ok_or(error)` turns an `Option` into a `Result`, so `?` can return that error.
  - `split_whitespace()` iterates over the space-separated words, and `next()` takes them one at a time.
  - `Duration::from_millis(cost.parse()?)`: `parse` works out from `from_millis`'s parameter that it's parsing a `u64`.
  - `map(String::from).collect()` turns the dependency words into owned `String`s: the `Task` must own its text, since it outlives the line it came from.
- **`Manifest`** keeps the tasks in a `BTreeMap<String, Task>`, sorted by name, so everything that walks them does so in the same order on every run.
  - **`tasks()`** returns `impl Iterator<Item = &Task>`: "some iterator of `&Task`". The real type is a `btree_map::Values` adapter; the caller doesn't need to know.
  - **`get(&str)`** looks up a `String` key with a `&str`. Maps allow any key form that borrows as the key type, so no `String` has to be made for a lookup.
- **Parsing a manifest** (also `FromStr`):
  - `text.lines()` iterates over the lines, and `enumerate()` numbers them from 0, so the line number is `index + 1`.
  - `split('#').next()` is the text before any comment. `split` always yields at least one piece, but `next()` returns an `Option` anyway; `unwrap_or_default()` gives `""` for `None`.
  - **`map_err`** transforms the error inside a `Result`: here, from a `TaskError` to a `ManifestError::BadLine` that adds the line number. Then `?` returns it.
  - **The `entry` API:** `tasks.entry(name)` looks the key up once, and returns an `Entry` that is `Occupied` or `Vacant`. A vacant entry's `insert` fills it without a second lookup.
  - **Unknown dependencies** are checked after every task is in, since a task may depend on one defined below it.

### Code
`lecture_5/src/manifest.rs`:
```rust
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;
use std::time::Duration;

// Tasks

// One line of a manifest: "name cost: dependency dependency ...", with the cost in milliseconds.
pub struct Task {
    pub name: String,
    pub cost: Duration,
    pub dependencies: Vec<String>,
}

// Everything that can be wrong with one line. Error requires Debug, so we derive it.
#[derive(Debug)]
pub enum TaskError {
    MissingColon,
    MissingName,
    MissingCost,
    ExtraField(String),
    BadCost(ParseIntError),
}

// Display is the text for people: what {} prints.
impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskError::MissingColon => write!(f, "expected a ':' after the cost"),
            TaskError::MissingName => write!(f, "expected a task name"),
            TaskError::MissingCost => write!(f, "expected a cost in milliseconds"),
            TaskError::ExtraField(field) => write!(f, "unexpected {field:?} before the ':'"),
            TaskError::BadCost(_) => write!(f, "the cost isn't a whole number of milliseconds"),
        }
    }
}

// Error marks a type as an error. source() returns the error that caused this one, if any.
impl Error for TaskError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TaskError::BadCost(error) => Some(error),
            _ => None,
        }
    }
}

// From lets ? turn a ParseIntError into a TaskError by itself.
impl From<ParseIntError> for TaskError {
    fn from(error: ParseIntError) -> TaskError {
        TaskError::BadCost(error)
    }
}

// FromStr is what str::parse calls: "app 40: engine ui".parse::<Task>().
impl FromStr for Task {
    type Err = TaskError;

    fn from_str(line: &str) -> Result<Task, TaskError> {
        // ok_or turns an Option into a Result, so ? can return the error.
        let (head, dependencies) = line.split_once(':').ok_or(TaskError::MissingColon)?;

        let mut fields = head.split_whitespace();
        let name = fields.next().ok_or(TaskError::MissingName)?;
        let cost = fields.next().ok_or(TaskError::MissingCost)?;
        if let Some(extra) = fields.next() {
            return Err(TaskError::ExtraField(extra.to_string()));
        }

        Ok(Task {
            name: name.to_string(),
            // parse() returns Result<u64, ParseIntError>, and ? converts the error with From.
            cost: Duration::from_millis(cost.parse()?),
            dependencies: dependencies.split_whitespace().map(String::from).collect(),
        })
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Duration's Debug prints it with a unit, like 40ms.
        write!(f, "{:<8} {:>5?}", self.name, self.cost)?;
        if !self.dependencies.is_empty() {
            write!(f, "  after {}", self.dependencies.join(", "))?;
        }
        Ok(())
    }
}

// Manifests

#[derive(Debug)]
pub enum ManifestError {
    BadLine { number: usize, error: TaskError },
    Duplicate(String),
    UnknownDependency { task: String, dependency: String },
    Cycle(Vec<String>),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::BadLine { number, .. } => write!(f, "line {number} is malformed"),
            ManifestError::Duplicate(name) => write!(f, "task {name:?} is defined twice"),
            ManifestError::UnknownDependency { task, dependency } => {
                write!(f, "task {task:?} depends on {dependency:?}, which isn't defined")
            }
            ManifestError::Cycle(names) => write!(f, "tasks {} wait on each other", names.join(", ")),
        }
    }
}

// The detail of a bad line is in its TaskError, so that's the source.
impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ManifestError::BadLine { error, .. } => Some(error),
            _ => None,
        }
    }
}

// The tasks by name. A BTreeMap keeps them sorted, so everything that walks them does so in the same order every run.
pub struct Manifest {
    tasks: BTreeMap<String, Task>,
}

impl Manifest {
    // impl Iterator: "some iterator over &Task". The caller can't name the real type, and doesn't need to.
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values()
    }

    // The keys are Strings, but a &str is enough to look one up.
    pub fn get(&self, name: &str) -> Option<&Task> {
        self.tasks.get(name)
    }
}

impl FromStr for Manifest {
    type Err = ManifestError;

    fn from_str(text: &str) -> Result<Manifest, ManifestError> {
        let mut tasks = BTreeMap::new();

        for (index, line) in text.lines().enumerate() {
            // Everything after a '#' is a comment. split always yields at least one piece.
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }

            // map_err wraps the line's error, adding the line number.
            let task: Task = line.parse().map_err(|error| ManifestError::BadLine { number: index + 1, error })?;

            // entry() looks the name up once, and says whether it's taken.
            match tasks.entry(task.name.clone()) {
                Entry::Occupied(_) => return Err(ManifestError::Duplicate(task.name)),
                Entry::Vacant(slot) => {
                    slot.insert(task);
                }
            }
        }

        // Every dependency must name a task.
        for task in tasks.values() {
            if let Some(dependency) = task.dependencies.iter().find(|dependency| !tasks.contains_key(*dependency)) {
                return Err(ManifestError::UnknownDependency { task: task.name.clone(), dependency: dependency.clone() });
            }
        }

        Ok(Manifest { tasks })
    }
}
```

## 5.3 Planning: `src/plan.rs`

### Why
This is where the collections earn their keep: each step of planning needs a different shape of data, and the standard library has a collection for each.

### How
- **Which collection:**
  - **`HashMap` and `HashSet`** answer "what's stored under this key?" and "is this in the set?" in constant time on average, by hashing. Their iteration order is arbitrary, and changes from run to run, so anything we *print* from one gets sorted first.
  - **`BTreeMap` and `BTreeSet`** keep their keys sorted, at a logarithmic cost per operation.
  - **`VecDeque`** is a ring buffer: pushing and popping at either end is cheap. It's the standard queue.
  - **`BinaryHeap`** always pops its largest element: a priority queue.
- **Final targets** are the tasks nothing depends on. `flat_map` collects every dependency of every task into one `HashSet<&str>`, and the targets are the tasks not in it. `String::as_str` turns each `&String` into a `&str`.
- **Build order, with Kahn's algorithm:** a task is ready once all of its dependencies are done.
  - **`waiting`** counts each task's unfinished dependencies. `collect()` builds a `HashMap` from an iterator of `(key, value)` pairs.
  - **`dependents`** reverses the edges: for each task, who is waiting on it. `entry(...).or_default()` inserts an empty `Vec` the first time a key is seen.
  - **`ready`** is a `VecDeque`, seeded with the tasks that have no dependencies. Tasks leave from the front, in the order they became ready, with `pop_front`, and join at the back with `push_back`.
  - **Finishing a task** decrements every dependent's count, and a dependent whose count reaches zero is ready. `get_mut` gives a `&mut usize` into the map to decrement in place.
  - **`.into_iter().flatten()` on an `Option<&Vec>`:** a task nobody depends on has no entry, so `get` returns `None`. An `Option` iterates over 0 or 1 values, so `flatten()` turns `None` into "no dependents" and `Some(list)` into the list's elements.
  - **Cycles:** if the order ends up shorter than the task list, some tasks never became ready: they're on a cycle, or wait on one. `into_iter()` on the `HashMap` consumes it, and the stuck names are sorted before they go into the error.
  - **Lifetimes:** `build_order` returns `Vec<&Task>`, references into the manifest, so the order can't outlive it. The `&str` keys of `waiting` and `dependents` borrow the manifest's names in the same way: no string is copied.
- **Waves:** every task's wave is one more than its latest dependency's, so a wave's tasks depend only on earlier waves, and can all run at once. Walking the build order guarantees every dependency already has a wave; `wave_of[key]` indexes the map, and panics if the key is missing, which would be a bug.
- **Explicit lifetimes:** `waves<'a>(order: &[&'a Task]) -> Vec<Vec<&'a Task>>` says the returned references live as long as the ones passed in, not as long as the slice holding them. With two references in the parameter, the compiler can't guess which one the result borrows from, so we name it.
- **The two-worker schedule** plays the order out on paper:
  - **`free`** holds each worker's next free time. `BinaryHeap` pops the largest element, and `Reverse(x)` flips the comparison, so it pops the *earliest* free time.
  - **Tuples compare field by field,** like words in a dictionary: `(Duration, usize)` orders by time, then by worker number, so ties go to the lower worker.
  - **Each task** goes to the worker that frees up first, and starts at the later of that worker's free time and its last dependency's finish time. `Duration` implements `Ord` and `+`, so times work like numbers.
  - **`Slot<'a>`** holds a `&'a Task`, so the struct needs the lifetime parameter too.

### Code
`lecture_5/src/plan.rs`:
```rust
use crate::manifest::{Manifest, ManifestError, Task};

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::time::Duration;

// Final targets

// The tasks nothing else depends on: what the manifest is ultimately for. A HashSet answers "is this in the set?" quickly, in any order.
pub fn final_targets(manifest: &Manifest) -> Vec<&str> {
    let needed: HashSet<&str> = manifest.tasks().flat_map(|task| task.dependencies.iter().map(String::as_str)).collect();
    manifest.tasks().map(|task| task.name.as_str()).filter(|name| !needed.contains(name)).collect()
}

// Build order

// Kahn's algorithm: a task is ready once all of its dependencies are done. Returns every task after its dependencies, or the tasks that can never become ready.
pub fn build_order(manifest: &Manifest) -> Result<Vec<&Task>, ManifestError> {
    // How many unfinished dependencies each task has. A HashMap: we only ever look names up, never walk them in order.
    let mut waiting: HashMap<&str, usize> = manifest.tasks().map(|task| (task.name.as_str(), task.dependencies.len())).collect();

    // The edges reversed: who is waiting on each task. or_default() inserts an empty Vec the first time.
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for task in manifest.tasks() {
        for dependency in &task.dependencies {
            dependents.entry(dependency.as_str()).or_default().push(&task.name);
        }
    }

    // A queue: tasks leave from the front in the order they became ready.
    let mut ready: VecDeque<&Task> = manifest.tasks().filter(|task| task.dependencies.is_empty()).collect();
    let mut order = Vec::with_capacity(waiting.len());

    while let Some(task) = ready.pop_front() {
        order.push(task);

        // A task nobody depends on has no entry. An Option iterates over 0 or 1 values, so flatten() turns None into an empty list.
        for &name in dependents.get(task.name.as_str()).into_iter().flatten() {
            let count = waiting.get_mut(name).expect("every dependent is a task");
            *count -= 1;
            if *count == 0 {
                ready.push_back(manifest.get(name).expect("every dependent is a task"));
            }
        }
    }

    if order.len() == waiting.len() {
        return Ok(order);
    }

    // Whatever still waits is on a cycle, or waits on one. HashMap order changes from run to run, so we sort.
    let mut stuck: Vec<String> = waiting.into_iter().filter(|&(_, count)| count > 0).map(|(name, _)| name.to_string()).collect();
    stuck.sort();
    Err(ManifestError::Cycle(stuck))
}

// Waves

// Groups the order into waves: each task's dependencies are all in earlier waves, so a wave's tasks can run at the same time.
pub fn waves<'a>(order: &[&'a Task]) -> Vec<Vec<&'a Task>> {
    let mut wave_of: HashMap<&str, usize> = HashMap::new();
    let mut waves: Vec<Vec<&Task>> = Vec::new();

    for &task in order {
        // One wave after its latest dependency. In build order, every dependency already has a wave; map[key] panics if one didn't.
        let wave = task.dependencies.iter().map(|dependency| wave_of[dependency.as_str()] + 1).max().unwrap_or(0);
        wave_of.insert(&task.name, wave);

        if wave == waves.len() {
            waves.push(Vec::new());
        }
        waves[wave].push(task);
    }

    waves
}

// Scheduling on paper

pub struct Slot<'a> {
    pub task: &'a Task,
    pub worker: usize,
    pub start: Duration,
    pub finish: Duration,
}

// Plays the build order out on `workers` workers: each task goes to the worker that frees up first, and starts once that worker is free and its dependencies have finished.
pub fn schedule<'a>(order: &[&'a Task], workers: usize) -> Vec<Slot<'a>> {
    // A BinaryHeap pops its largest element. Reverse flips the comparison, so it pops the earliest free time. Tuples compare field by field, so ties go to the lower worker number.
    let mut free: BinaryHeap<Reverse<(Duration, usize)>> = (0..workers).map(|worker| Reverse((Duration::ZERO, worker))).collect();
    let mut finished: HashMap<&str, Duration> = HashMap::new();
    let mut slots = Vec::with_capacity(order.len());

    for &task in order {
        let Reverse((free_at, worker)) = free.pop().expect("there is at least one worker");
        let ready_at = task.dependencies.iter().map(|dependency| finished[dependency.as_str()]).max().unwrap_or(Duration::ZERO);

        let start = free_at.max(ready_at);
        let finish = start + task.cost;

        finished.insert(&task.name, finish);
        free.push(Reverse((finish, worker)));
        slots.push(Slot { task, worker, start, finish });
    }

    slots
}
```

## 5.4 Threads: `src/run.rs`

### Why
The waves say which tasks *could* run at once; now we run them that way, on real threads, and check the parallel run takes about as long as the waves predict. Each task also produces a result, an artifact hash, that depends on its dependencies' results, the way a build cache's keys do.

### How
- **A hasher of our own:**
  - **The `Hasher` trait** is what `HashMap` uses to hash keys. A hasher is fed bytes with `write`, and `finish` returns the hash. Anything that implements `Hash` (strings, numbers, tuples, `Vec`s, and any struct that derives it) knows how to feed itself to any `Hasher`: `task.name.hash(&mut hasher)`.
  - **FNV-1a** is a tiny, well-known hash function: for each byte, XOR it into the state, then multiply by a fixed prime. `wrapping_mul` says the overflow is intended. We write our own because we print these hashes: `HashMap`'s default hasher starts from random keys on every run (so no one can craft keys that all collide), and its algorithm may change between Rust versions.
  - **`Default`** gives the starting state, as `Fnv1a::default()`. It's what `HashMap` would use to make a fresh hasher too.
- **Artifact hashes:** a task's hash covers its name and its dependencies' hashes, in order. Change anything upstream, and every hash downstream changes.
- **Threads:**
  - **`thread::scope(|scope| ...)`** runs a closure that may spawn threads with `scope.spawn`, and joins every one of them before `scope` returns. Because the threads can't outlive the scope, they may borrow local variables: the tasks, `hashes`, `busy`.
  - **Rust checks the sharing:** a value may be shared between threads only if its type is `Sync`, and moved to another thread only if it's `Send`. `HashMap<String, u64>` is `Sync`, so all the threads can read it through `&hashes` at once. `Rc` (lecture 2) is neither, which is why it can't be used here, and `Arc`, its thread-safe twin, can.
  - **`move ||`** makes the closure take ownership of what it captures: its own `Sender`, and copies of the references `hashes`, `busy` and `task`.
  - **No data races:** `hashes` is only read during a wave, and only extended between waves, after `scope` has joined every thread. Writing to it inside a thread would not compile, because it's shared.
- **Channels:** `mpsc::channel()` returns a `Sender` and a `Receiver` ("multiple producer, single consumer"). Each thread gets a clone of the `Sender`, and sends its `(name, hash)` back.
  - **The end of the stream:** iterating over a `Receiver` waits for messages until every `Sender` has been dropped. The threads' clones are gone once `scope` returns, and `drop(sender)` gets rid of ours.
  - **`hashes.extend(receiver)`** inserts every message into the map: a `HashMap` can be extended from any iterator of pairs, and a `Receiver` is one.
- **`Mutex<Duration>`** lets the threads add to one shared total.
  - **`lock()`** waits until no other thread holds the lock, and returns a *guard*: a smart pointer to the `Duration` inside. The lock is released when the guard is dropped, here at the end of the statement. That's lecture 2's `Drop`, guarding a lock.
  - **Poisoning:** `lock()` returns a `Result`, which is an error only if another thread panicked while holding the lock. We don't expect that, so `expect` says so.
  - **`into_inner()`** consumes the `Mutex` and returns the value, once no thread can use it any more.
- **`Instant::now()`** reads a monotonic clock, and `elapsed()` returns the `Duration` since. `thread::sleep(task.cost)` stands in for real work.

### Code
`lecture_5/src/run.rs`:
```rust
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
```

## 5.5 The program: `src/main.rs`

### Why
`main` reads a manifest, from a file if one is named on the command line, prints every step of the plan, runs it, saves the schedule, and finally shows the errors for five broken manifests.

### How
- **Raw strings:** `r#"..."#` contains no escapes, and can span lines, so the manifest is written as it would appear in a file.
- **`check`** shows `?` across two error sources: `text.parse()?` and `build_order(...)?` both return `ManifestError`s, so they pass straight through.
- **`print_error`** takes `&dyn Error`, so it accepts any error type, and walks the `source()` chain with `while let`.
- **Writing a file:**
  - **`BufWriter`** wraps the `File`, and collects small writes into big ones, so each `writeln!` doesn't become a system call.
  - **`writeln!`** formats into anything that implements `io::Write`, and returns an `io::Result`, so each line ends with `?`.
  - **`flush()`:** dropping a `BufWriter` flushes it too, but a `drop` can't return an error, so it's ignored there. Calling `flush()` ourselves reports it.
- **Reading it back:**
  - **`BufReader::lines()`** yields one `io::Result<String>` per line, since reading can fail halfway through. `skip(1)` drops the header.
  - **`split(',').nth(3)`** is the fourth field, the finish time, or `None` if the row has fewer fields; `ok_or` turns that into an error with a message.
  - **`Box<dyn Error>`** can hold any error. `?` converts each error into one with `From`: an `io::Error`, a `ParseIntError`, and even a plain `&str` message all convert. That makes `Box<dyn Error>` the easy choice for application code, where errors are reported rather than handled one by one; libraries use specific error types like ours, so callers can `match` on them.
- **`main` returns `Result<(), Box<dyn Error>>`.** Every `?` in `main` works, and an `Err` ends the program with exit code 1, printing the error's `Debug` form.
- **The command line:** `std::env::args()` iterates over the program's arguments, the program's own name first, so `nth(1)` is the first real argument. `fs::read_to_string(path)?` reads a whole file into a `String`.
- **`Duration`:** `sum()` adds up an iterator of them, `unwrap_or_default()` turns an empty wave's `max()` of `None` into zero, and `{:?}` prints them with units.
- **Paths:** `std::env::temp_dir()` is the system's directory for temporary files, as a `PathBuf`; `join` appends a file name with the right separator, and `display()` prints a path that may not be valid UTF-8.
- **`BTreeMap<&String, &u64>`:** collecting the `HashMap`'s pairs into a `BTreeMap` sorts them by name, so the output is the same on every run.

### Code
`lecture_5/src/main.rs`:
```rust
mod manifest;
mod plan;
mod run;

use manifest::{Manifest, ManifestError};
use plan::{Slot, build_order, final_targets, schedule, waves};

use std::collections::BTreeMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::time::Duration;

// A raw string, r#"..."#: no escapes, and it can span lines.
const MANIFEST: &str = r#"
# name   cost (ms): dependencies
math     10:
gpu      25: math
ui       20: math
assets   35:
engine   30: math gpu
app      40: engine ui
tests    15: app
package   5: app assets
"#;

// Manifests that fail, one way each.
const BAD_MANIFESTS: [&str; 5] = ["math 10", "math ten:", "math 10:\nmath 12:", "app 40: engine", "a 1: b\nb 1: c\nc 1: a\nd 1:"];

// Errors

// Parses a manifest and orders it. Each ? returns the error early, converted with From if needed: here both are already ManifestErrors.
fn check(text: &str) -> Result<usize, ManifestError> {
    let manifest: Manifest = text.parse()?;
    Ok(build_order(&manifest)?.len())
}

// Prints an error, then each error that caused it. &dyn Error accepts any error type.
fn print_error(error: &dyn Error) {
    println!("    error: {error}");
    let mut source = error.source();
    while let Some(cause) = source {
        println!("      caused by: {cause}");
        source = cause.source();
    }
}

// Files

// BufWriter gathers many small writes into a few big ones; writeln! formats straight into it.
fn write_report(path: &Path, slots: &[Slot]) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(out, "task,worker,start_ms,finish_ms")?;
    for slot in slots {
        writeln!(out, "{},{},{},{}", slot.task.name, slot.worker, slot.start.as_millis(), slot.finish.as_millis())?;
    }

    // Dropping a BufWriter flushes it, but has to ignore any error. flush() reports it.
    out.flush()
}

// Reads the report back, line by line, and returns the latest finish time in it.
fn read_report(path: &Path) -> Result<(usize, u64), Box<dyn Error>> {
    let reader = BufReader::new(File::open(path)?);
    let mut rows = 0;
    let mut latest = 0;

    // skip(1) drops the header. Each line is an io::Result<String>, since reading can fail halfway.
    for line in reader.lines().skip(1) {
        let line = line?;
        let finish = line.split(',').nth(3).ok_or("a row without a finish column")?;

        // A ParseIntError and a &str (the message above) both convert into Box<dyn Error>, so ? works on either.
        latest = latest.max(finish.parse()?);
        rows += 1;
    }

    Ok((rows, latest))
}

// The program

// main may return a Result. An Err ends the program with exit code 1, printing the error's Debug form.
fn main() -> Result<(), Box<dyn Error>> {
    // With a file name on the command line, the manifest comes from that file instead.
    let text = match std::env::args().nth(1) {
        Some(path) => fs::read_to_string(path)?,
        None => MANIFEST.to_string(),
    };

    let manifest: Manifest = text.parse()?;
    println!("Tasks:");
    for task in manifest.tasks() {
        println!("    {task}");
    }
    println!("Final targets: {}", final_targets(&manifest).join(", "));

    let order = build_order(&manifest)?;
    let names: Vec<&str> = order.iter().map(|task| task.name.as_str()).collect();
    println!("Build order: {}", names.join(", "));

    // Waves, and how long each would take with every task in it running at once.
    let waves = waves(&order);
    let mut wave_total = Duration::ZERO;
    println!("Waves:");
    for (number, wave) in waves.iter().enumerate() {
        let longest = wave.iter().map(|task| task.cost).max().unwrap_or_default();
        wave_total += longest;
        let names: Vec<&str> = wave.iter().map(|task| task.name.as_str()).collect();
        println!("    {number}: {:<16} {longest:?}", names.join(", "));
    }

    // Sum works on any iterator of Durations.
    let sequential: Duration = order.iter().map(|task| task.cost).sum();
    println!("One task at a time: {sequential:?}; one thread per task in each wave: {wave_total:?}");

    let slots = schedule(&order, 2);
    println!("Two workers:");
    println!("    {:<8} {:>6} {:>6} {:>6}", "task", "worker", "start", "finish");
    for slot in &slots {
        println!("    {:<8} {:>6} {:>6} {:>6}", slot.task.name, slot.worker, slot.start.as_millis(), slot.finish.as_millis());
    }

    // Running the waves on real threads.
    let run = run::run(&waves);

    // Collecting into a BTreeMap sorts by name; the HashMap's own order would change from run to run.
    let sorted: BTreeMap<&String, &u64> = run.hashes.iter().collect();
    println!("Threads:");
    for (name, hash) in &sorted {
        println!("    {name:<8} {hash:016x}");
    }
    println!("    busy {:?} across all threads, done in {} ms", run.busy, run.elapsed.as_millis());

    // temp_dir() is the system's directory for temporary files; join adds a file name with the right separator.
    let path = std::env::temp_dir().join("lecture_5_schedule.csv");
    write_report(&path, &slots)?;
    let (rows, latest) = read_report(&path)?;
    println!("Wrote {} ({} bytes), read back {rows} rows finishing at {latest} ms", path.display(), fs::metadata(&path)?.len());

    println!("Bad manifests:");
    for text in BAD_MANIFESTS {
        println!("    {text:?}");
        if let Err(error) = check(text) {
            print_error(&error);
        }
    }

    Ok(())
}
```

## 5.6 Build and run

```bash
cargo run -p lecture_5
```

```text
Tasks:
    app       40ms  after engine, ui
    assets    35ms
    engine    30ms  after math, gpu
    gpu       25ms  after math
    math      10ms
    package    5ms  after app, assets
    tests     15ms  after app
    ui        20ms  after math
Final targets: package, tests
Build order: assets, math, gpu, ui, engine, app, package, tests
Waves:
    0: assets, math     35ms
    1: gpu, ui          25ms
    2: engine           30ms
    3: app              40ms
    4: package, tests   15ms
One task at a time: 180ms; one thread per task in each wave: 145ms
Two workers:
    task     worker  start finish
    assets        0      0     35
    math          1      0     10
    gpu           1     10     35
    ui            0     35     55
    engine        1     35     65
    app           0     65    105
    package       1    105    110
    tests         0    105    120
Threads:
    app      f30684eae55bfdbc
    assets   7eedbf5926369625
    engine   ea058489bdafb320
    gpu      13d6cc6b91839695
    math     c3a256a4fe33d9d0
    package  4659c510997a472e
    tests    2dac08935c416529
    ui       0060af70f0ab5d9f
    busy 180ms across all threads, done in 146 ms
Wrote /tmp/lecture_5_schedule.csv (142 bytes), read back 8 rows finishing at 120 ms
Bad manifests:
    "math 10"
    error: line 1 is malformed
      caused by: expected a ':' after the cost
    "math ten:"
    error: line 1 is malformed
      caused by: the cost isn't a whole number of milliseconds
      caused by: invalid digit found in string
    "math 10:\nmath 12:"
    error: task "math" is defined twice
    "app 40: engine"
    error: task "app" depends on "engine", which isn't defined
    "a 1: b\nb 1: c\nc 1: a\nd 1:"
    error: tasks a, b, c wait on each other
```

The `done in ... ms` figure changes a little from run to run; everything else is the same every time.

Reading the output:
- **Tasks** print in name order, because the manifest is a `BTreeMap`.
- **The build order** starts with the two tasks that depend on nothing, `assets` and `math`, and every task comes after its dependencies.
- **The plans:** one task at a time takes 180 ms. One thread per task in each wave takes 145 ms, the sum of each wave's longest task. Two workers finish sooner, at 120 ms: the length of the longest chain of dependencies (`math`, `gpu`, `engine`, `app`, `tests`). Each task starts as soon as its own dependencies are done, without waiting for a whole wave, and two workers are always enough to keep that chain moving.
- **The threads** were busy for 180 ms in total, the same work as the sequential plan, yet finished a little over 145 ms, as the waves predicted: sleeps overshoot slightly, and starting threads takes a moment, in every wave. Rename `math` and every hash downstream of it changes, which is all of them but `assets`; rename `tests`, and only its own changes.
- **The file** has a header and 8 rows, and its latest finish matches the two-worker plan.
- **The bad manifests:** each error says what went wrong, and `caused by` lines follow the `source()` chain. A bad cost is three errors deep: the line, the task, and the integer parser's own message. The cycle names `a`, `b` and `c` but not `d`, which has no dependencies.

In lecture 6 we leave the standard library behind and call code written in C and C++: our own small library, compiled by Cargo, with Rust bindings generated from its header.
