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
