use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

// A single unit of concurrent execution
struct Task;
// An immutable data-structure to store all constant data needed at runtime
struct ConstantTable;

// A reference to a given worker
struct WorkerHandle {
  outbox     : Sender<WorkerCommand>,
  task_count : u32,
}

// Status reports that the worker will report back to the scheduler
enum WorkerReport {
  Dead,
  Asleep,
  Available,
  Okay
}

// Commands that the scheduler can send the worker
enum WorkerCommand {
  SpawnTask( Task ),
  Die
}

// A concurrent task manager for it's own thread
struct Worker<'a> {
  rewrites : u32,
  tasks    : Vec<Task>,
  current_task : Option<&'a mut Task>
}

// Keeps tracks of the workers by spawning, destroying and sending them tasks
struct Scheduler {
  worker_handles : Vec<WorkerHandle>,
  pool_max_size  : u32,
  rewrite_quota  : u32,
  inbox          : Receiver<WorkerReport>
}

// This is the central data-structure that containts everything needed at
// runtime, which is the scheduler, constants and possibly a garbage collector
struct Runtime {
  schedulers : Scheduler,
  constants  : Arc<ConstantTable>
}