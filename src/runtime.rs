use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::thread;
use ::num_cpus;

/*
  Runtime model:
  - The runtime is created spawning a scheduler
  - The scheduler will then create a thread with a worker and spawn a task
    running the code at it's entry point
*/

// The representation of functions
#[derive(Clone)]
pub struct Function {
  pub code  : Code,
  pub arity : u32
}

// An operation that can be executed
#[derive(Clone)]
pub enum Op {
  Nop,
  Push( TaggedValue ),
  Pop,
  Invoke( TaggedValue ),
  PrintStack
}

pub type ConstRef = u32;

#[derive(Clone)]
pub enum ConstVal {
  Function( Function ),
  Atom( String )
}

// The representation of all values in Stackpile (either immidiate or indirect) 
#[derive(Clone)]
pub enum TaggedValue {
  Int( i32 ),
  Float( f32 ),
  Bool( bool ),
  Const( ConstRef )
}

// A tasks execution stack
pub type Stack = Vec<TaggedValue>;

// Representation of the code of a given task
pub type Code = Vec<Op>;

// A single unit of concurrent execution
pub struct Task {
  stack : Stack,
  code  : Code,
}

impl Task {
  pub fn new( code : Code ) -> Task {
    Task { code : code
         , stack: Vec::new() }
  }
}

// An immutable data-structure to store all constant data needed at runtime
pub struct ConstantTable {
  pub names     : Vec<(String, ConstRef)>,
  pub constants : Vec<ConstVal>
}

impl ConstantTable {
  // Get's the main function ( if defined )
  fn main( &self ) -> Code {
    // TODO: Probably shouldn't use a vector due to unnecesarry allocation
    let mut mmain : Vec<_> = 
      self.names.iter()
                .filter( |&&( ref n, _ )| n == "main" )
                .collect();

    // TODO: Don't panic but actually return error
    if mmain.len() != 1 {
      panic!( "Expected one main function!" )
    }

    // Take the looked up atom and fetch the code from it
    let mcode = &self.constants[mmain.pop().unwrap().1 as usize];

    match mcode {
      &ConstVal::Function( ref f ) => f.code.clone(),
      _ => panic!( "Expected main to be a function" )
    }
  }
}

// A reference to a given worker
pub struct WorkerHandle {
  outbox     : Sender<WorkerCommand>,
  task_count : u32,
}

// Status reports that the worker will report back to the scheduler
pub enum WorkerReport {
  Dead,
  Asleep,
  Available,
  Okay
}

// Commands that the scheduler can send the worker
pub enum WorkerCommand {
  SpawnTask( Task ),
  Die
}

// A concurrent task manager for it's own thread
pub struct Worker<'a> {
  outbox       : Sender<WorkerReport>,
  inbox        : Receiver<WorkerCommand>,
  rewrites     : u32,
  tasks        : Vec<Task>,
  current_task : Option<&'a mut Task>
}

impl<'a> Worker<'a> {
  fn new( inbox : Receiver<WorkerCommand>, outbox : Sender<WorkerReport> )
     -> Worker<'a> {
 
    Worker { outbox  : outbox
           , inbox   : inbox
           , rewrites: 0
           , tasks   : Vec::new()
           , current_task : None }
  }

  fn run( &mut self ) {
    // TODO
  }
}

// Keeps tracks of the workers by spawning, destroying and sending them tasks
pub struct Scheduler {
  worker_handles : Vec<WorkerHandle>,
  task_count     : u32,
  pool_max_size  : u32,
  rewrite_quota  : u32,
  inbox          : Receiver<WorkerReport>,
  outbox_seed    : Sender<WorkerReport>
}

impl Scheduler {
  fn new( pool_size : u32, rwq : u32 ) -> Scheduler {
    let (o, i) = channel();

    Scheduler { worker_handles : Vec::new()
              , task_count     : 0
              , pool_max_size  : pool_size
              , rewrite_quota  : rwq
              , inbox          : i
              , outbox_seed    : o }
  }

  fn spawn_worker( &mut self ) -> &mut WorkerHandle {
    let (wo, wi) = channel();
    let mut worker = Worker::new( wi, self.outbox_seed.clone() );

    // Spawn the worker in it's own thread
    thread::spawn( move || worker.run() );

    // Create and register a handle for the worker
    let worker_handle = WorkerHandle { task_count: 0, outbox: wo };
    self.worker_handles.push( worker_handle );
    
    // And return a reference to that handle
    let lidx = self.worker_handles.len() - 1;
    &mut self.worker_handles[lidx]
  }

  fn get_available_worker( &mut self ) -> &mut WorkerHandle {
    // Worker count
    let wc = self.worker_handles.len() as u32;

    // TODO: this could be balanced better
    // Spawn a new worker until we've reached the pool limit
    if wc == 0 || ( wc < self.pool_max_size ) {
      self.spawn_worker()
    } else {
      // Find the worker with the least tasks
      self.worker_handles
          .iter_mut()
          .fold( None
               , | best, wrker | {
                 match best {
                   // The first one is our inital best
                   None => Some( wrker ),
                   Some( bwrker ) => Some(
                     // Otherwise pick the one with the lowest
                     // task count.
                     if wrker.task_count < bwrker.task_count {
                       wrker
                     } else {
                       bwrker
                     }
                   )
                 }
               } )
          // We checked earlier, so this should never be None
          // And if it is, it's better just to crash
          .unwrap()
    }
  }

  fn spawn_task( &mut self, task : Task ) {
    {
      let wh = self.get_available_worker();

      wh.outbox.send( WorkerCommand::SpawnTask( task ) );
      wh.task_count += 1;
    }
    self.task_count += 1;
  }
}

pub struct RuntimeSettings {
  pub worker_pool_size : u32,
  pub rewrite_quota    : u32,
}

impl Default for RuntimeSettings {
  fn default() -> RuntimeSettings {
    RuntimeSettings { worker_pool_size: (num_cpus::get() - 1) as u32
                    , rewrite_quota   : 32 }
  }
}

// This is the central data-structure that containts everything needed at
// runtime, which is the scheduler, constants and possibly a garbage collector
pub struct Runtime {
  scheduler : Scheduler,
  constants : Arc<ConstantTable>,
}

impl Runtime {
  pub fn new( settings : RuntimeSettings, ct : ConstantTable ) -> Runtime {
    
    let scheduler =
      Scheduler::new( settings.worker_pool_size
                    , settings.rewrite_quota );
    
    Runtime { scheduler: scheduler
            , constants: Arc::new( ct ) }
  }

  pub fn run( &mut self ) {
    self.scheduler.spawn_task( Task::new( self.constants.main() ) );
  }
}
