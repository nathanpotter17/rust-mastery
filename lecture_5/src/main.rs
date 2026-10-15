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
