use std::sync::mpsc::{Receiver, Sender, channel, TryRecvError};
use std::sync::Arc;
use std::thread;
use ::num_cpus;

// TODO: Split this up into smaller files

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
#[derive(Clone, Debug)]
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

enum TaskStatus {
  Ok,
  Dead,
  Yielded
}

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

  pub fn step( &mut self ) -> TaskStatus {
    // TODO: Move this execution part to it's own
    match self.code.pop().unwrap() {
      Op::Nop => {},
      Op::Push( v ) => self.stack.push( v ),
      Op::Pop => { self.stack.pop(); },
      Op::Invoke( v ) => unimplemented!(),
      Op::PrintStack => println!( "{:?}", self.stack )
    }

    if self.code.is_empty()  {
      TaskStatus::Dead
    } else {
      TaskStatus::Ok
    }
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

type WorkerId = u32;

// A reference to a given worker
pub struct WorkerHandle {
  id         : WorkerId,
  outbox     : Sender<WorkerCommand>,
  task_count : u32,
  handle     : thread::JoinHandle<()>
}

// Status reports that the worker will report back to the scheduler
pub enum WorkerReport {
  Dead( WorkerId ),
  Finsihed( WorkerId ),
  SpawnTask( Task )
}

// Commands that the scheduler can send the worker
pub enum WorkerCommand {
  SpawnTask( Task ),
  Die
}

// A concurrent task manager for it's own thread
pub struct Worker {
  id            : WorkerId,
  outbox        : Sender<WorkerReport>,
  inbox         : Receiver<WorkerCommand>,
  rewrites      : u32,
  rewrite_quota : u32,
  tasks         : Vec<Task>,
  alive         : bool,
  current_task  : usize
}

impl Worker {
  fn new( id : WorkerId, inbox : Receiver<WorkerCommand>
        , outbox : Sender<WorkerReport>, quota : u32 ) -> Worker {
 
    Worker { id           : id
           , outbox       : outbox
           , inbox        : inbox
           , rewrites     : 0
           , rewrite_quota: quota
           , tasks        : Vec::new()
           , alive        : true
           , current_task : 0 }
  }

  fn run( &mut self ) {
    self.next_task();

    while self.alive {

      match self.step_current_task() {
        TaskStatus::Ok => self.tick_task(),
        TaskStatus::Dead => self.remove_current_task(),
        TaskStatus::Yielded => self.next_task()
      }

      self.check_for_interrupt();
    }

    self.shutdown();
  }

  fn tick_task( &mut self ) {
    self.rewrites += 1;

    if self.rewrites == self.rewrite_quota {
      self.rewrites = 0;
    }
  }

  fn next_task( &mut self ) {
    // If there's no tasks, wait for one
    if self.tasks.len() == 0 {
      self.wait_for_task();
      self.current_task = 0;
      // Check if we've been killed while waitng for the next task
      if !self.alive {
        return
      }
    }
    // Get the next task in a vector ( cyclic )
    self.current_task = ( self.current_task + 1 ) % self.tasks.len();
  }

  fn wait_for_task( &mut self ) {
    // Unwrap because if this call fails, our scheduler has died, and 
    // at that point, does anything really matter...?
    match self.inbox.recv().unwrap() {
      WorkerCommand::SpawnTask( t ) => {
        self.tasks.push( t );
      },
      WorkerCommand::Die => {
        self.alive = false; // :(
      }
    }
  }

  fn step_current_task( &mut self ) -> TaskStatus {
    let task = &mut self.tasks[self.current_task];
    task.step()
  }

  // Check if there's a command sent to the worker
  // if there's no active tasks, we block and wait
  fn check_for_interrupt( &mut self ) {
    if !self.alive { return }

    match self.inbox.try_recv() {
      Ok( WorkerCommand::SpawnTask( t ) ) => {
        self.tasks.push( t );
      },
      Ok( WorkerCommand::Die ) => {
        self.alive = false; // :(
      },
      Err( TryRecvError::Disconnected ) =>
        panic!( "Worker got disconneced from scheduler!" ),
      // There's no new command, so ignore it
      _ => {}
    }
  }

  fn remove_current_task( &mut self ) {
    // TODO: Do we actually care about task order?
    //       If yes then we use remove() else swap_remove()
    self.tasks.remove( self.current_task );

    // Our current task index is automatically pointing to the next
    // task after a we remove one, but we have to make sure it's in bounds:
    self.current_task = if self.tasks.len() <= 1 {
      0
    } else {
      self.current_task - 1
    };

    // Tell the scheduler we finished a task
    self.outbox.send( WorkerReport::Finsihed( self.id ) );

    // And switch to the next task
    self.next_task();
  }

  fn shutdown( &mut self ) {
    // TODO: Shutdown the task properly
    self.outbox.send( WorkerReport::Dead( self.id ) );
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

    let wid = self.worker_handles.len() as WorkerId;

    let mut worker =
      Worker::new( wid, wi, self.outbox_seed.clone(), self.rewrite_quota );

    // Spawn the worker in it's own thread
    let h = thread::spawn( move || { worker.run() } );

    // Create and register a handle for the worker
    let worker_handle =
      WorkerHandle { id : wid, task_count: 0, outbox: wo, handle : h };

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
    // We borrow self mutably here so we scope it -
    {
      let wh = self.get_available_worker();

      wh.outbox.send( WorkerCommand::SpawnTask( task ) );
      wh.task_count += 1;
    }
    // - so we can mutate it afterwards
    self.task_count += 1;
  }

  fn run( &mut self ) {
    // Keep managing the workers until all workers have finished their tasks
    while self.task_count != 0 {
      self.wait_for_worker_report();
    }
    self.shutdown_workers();
  }

  fn wait_for_worker_report( &mut self ) {

    // Unwrap since if the worker has paniced we shall too
    match self.inbox.recv().unwrap() {
      WorkerReport::Finsihed( wid ) => self.worker_finished( wid ),
      WorkerReport::Dead( wid ) => self.worker_crashed( wid ),
      WorkerReport::SpawnTask( t ) => self.spawn_task( t )
    }
  }

  fn worker_finished( &mut self, wid : WorkerId ) {
    // Find the worker with the given ID
    for wh in self.worker_handles.iter_mut() {
      if wh.id == wid {
        // Since it finished the task we decrement it's counter
        wh.task_count -= 1;
        self.task_count -= 1;
      }
    }
  }

  fn worker_crashed( &mut self, wid : WorkerId ) {
    // TODO: We might want to actually handle this properly
    panic!( "Worker {} crashed/ended unexpectedly!", wid )
  }

  fn shutdown_workers( &mut self ) {
    // Tell them all to die
    for worker in self.worker_handles.drain( .. ) {
      worker.outbox.send( WorkerCommand::Die );
      // Then wait for the thread to stop
      worker.handle.join();
      // Even though the workers will probably not shut down in the order we're
      // joining their thread, it doesn't really matter since we still have to
      // wait for the slowest thread to end before we stop execution.
    }
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
    self.scheduler.run();
  }
}
