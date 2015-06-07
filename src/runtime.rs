use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::thread;
use std::mem::{size_of, transmute};
use ::num_cpus;

use task::{Task, Code};
use worker::{Worker, WorkerId, WorkerCommand, WorkerReport, WorkerHandle};

// The index type of the constant table
pub type ConstRef = usize;

// An immutable data-structure to store all constant data needed at runtime
pub struct ConstantTable {
  pub names     : Vec<(String, ConstRef)>,
  pub constants : Vec<u8>
}

impl ConstantTable {
  // Get's the main function ( if defined )
  pub fn main( &self ) -> Code {
    // TODO: Probably shouldn't use a vector due to unnecesarry allocation
    let mut mmain : Vec<_> = 
      self.names.iter()
                .filter( |&&( ref n, _ )| n == "main" )
                .collect();

    // TODO: Don't panic but actually return error
    if mmain.len() != 1 {
      panic!( "Expected one main function!" )
    }

    self.get_code( mmain.pop().unwrap().1 as usize )
  }


  // Gets an arbitary copiable type from the raw bytes
  pub fn get<T : Copy>( &self, idx : usize ) -> T {
    assert!( idx < self.constants.len() );

    let ff_size = size_of::<T>();

    assert!( idx + ff_size <= self.constants.len() );

    let ff : T;
    unsafe {
      ff =
        *transmute::<_, &T>( &self.constants[idx] as *const u8 );
    }

    ff
  }

  // Get a chunked peice of a data ( 32 bit length field followed by it's contents )
  pub fn get_chunk( &self, idx : usize ) -> &[u8] {
    // Make sure the index in is bounds
    assert!( idx < self.constants.len() );
    // And make sure we got space for the SIZE field
    assert!( idx + 4 <= self.constants.len() );

    let size : u32;
    unsafe {
      // Read the u32 size field
      size = *transmute::<_, &u32>( &self.constants[idx] as *const u8 );
    }

    // Make sure the chunk data is inside bounds too
    assert!( idx + 4 + (size as usize) <= self.constants.len() );

    let low  = idx + 4;
    let high = idx + 4 + (size as usize);

    &self.constants[low..high]
  }

  // Reads a raw chunk and turns it into a code structure
  pub fn get_code( &self, idx : usize ) -> Code {
    let code_bytes = self.get_chunk( idx );
    Code::new( code_bytes.to_vec() )
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
    self.scheduler.spawn_task( Task::new( self.constants.main()
                                        , self.constants.clone() ) );
    self.scheduler.run();
  }
}
