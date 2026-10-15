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
