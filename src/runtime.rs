use std::sync::mpsc::{Receiver, Sender, channel, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::mem::{size_of, transmute};
use ::num_cpus;

// TODO: Split this up into smaller files

type ForeignFunction = fn( &mut Task ) -> TaskStatus;
type ForeignMap      = fn( u32, Arc<ConstantTable> ) -> u32;
type ForeignBinop    = fn( u32, u32, Arc<ConstantTable> ) -> u32;

#[derive(Debug, Clone)]
enum OpDecodeError {
  UnknownOp( u8, usize ),
  NoArgument( u8, usize )
}

// An operation that can be executed
#[derive(Copy, Clone, Debug)]
pub enum Op {
  Nop,
  Yield,
  Spawn( ConstRef ),
  If( usize, ConstRef, ConstRef ),
  Call( usize, ConstRef ),
  Callf( usize, ConstRef ),
  Map( usize, ConstRef ),
  Mapto( usize, usize, ConstRef ),
  Binop( usize, ConstRef ),
  Callv( usize, usize ),

  Swap( usize, usize ),
  Move( usize, usize ),
  Set( usize, u32 ),
  Cursor( isize )
}

struct OpCursor<'a> {
  index : usize,
  code  : &'a [u8]
}

impl<'a> OpCursor<'a> {
  fn next_u32( &mut self, op : u8 ) -> Result<usize, OpDecodeError> {
    // Make sure there's actually a u32 available, if not
    if self.index < 4 {
      // Error out with the op that required it and where it railed
      Err( OpDecodeError::NoArgument( op, self.index ) )
    } else {
      self.index -= 4;
      let v : &u32;
      unsafe {
        // Transmute the the &[u8; 4] to a &u32 since they have the same width
        v = transmute( &self.code[self.index] as *const u8 );
      }
      Ok( (*v) as usize )
    }
  }

  fn next_u8( &mut self, op : u8 ) -> Result<usize, OpDecodeError> {
    if self.index == 0 {
      Err( OpDecodeError::NoArgument( op, self.index ) )
    } else {
      self.index -= 1;
      Ok( self.code[self.index] as usize )
    }
  }

  fn next_i16( &mut self, op : u8 ) -> Result<isize, OpDecodeError> {
    // Make sure there's actually a u32 available, if not
    if self.index < 2 {
      // Error out with the op that required it and where it railed
      Err( OpDecodeError::NoArgument( op, self.index ) )
    } else {
      self.index -= 2;
      let v : &i16;
      unsafe {
        // Transmute the the &[u8; 4] to a &u32 since they have the same width
        v = transmute( &self.code[self.index] as *const u8 );
      }
      Ok( (*v) as isize )
    }
  }
}

impl Op {
  fn decode( code : &[u8] ) -> Result<(Op, usize), OpDecodeError> {
    let mut cursor = OpCursor { index: code.len() - 1, code: code };
    let op = code[code.len() - 1];
    Ok( match op {
      0x00 => (Op::Nop, 1),
      0x01 => (Op::Yield, 1),
      0x02 => {
        let val = try!( cursor.next_u32( op ) );
        (Op::Spawn( val ), 1 + 4)
      },
      0x03 => {
        let x = try!( cursor.next_u8( op ) );
        let t = try!( cursor.next_u32( op ) );
        let e = try!( cursor.next_u32( op ) );
        (Op::If( x, t, e ) , 1 + 1 + 4 + 4 )
      },
      0x04 => {
        let x = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        (Op::Call( x, f ) , 1 + 1 + 4 )
      },
      0x05 => {
        let x = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        (Op::Callf( x, f ) , 1 + 1 + 4 )
      },
      0x06 => {
        let x = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        (Op::Map( x, f ) , 1 + 1 + 4 )
      },
      0x07 => {
        let x = try!( cursor.next_u8( op ) );
        let y = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        (Op::Mapto( x, y, f ) , 1 + 1 + 1 + 4 )
      },
      0x08 => {
        let x = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        (Op::Binop( x, f ) , 1 + 1 + 4 )
      },
      0x09 => {
        let x = try!( cursor.next_u8( op ) );
        let v = try!( cursor.next_u8( op ) );
        (Op::Callv( x, v ) , 1 + 1 + 1 )
      },
      0x20 => {
        let x = try!( cursor.next_u8( op ) );
        let y = try!( cursor.next_u8( op ) );
        (Op::Swap( x, y ) , 1 + 1 + 1 )
      },
      0x21 => {
        let x = try!( cursor.next_u8( op ) );
        let y = try!( cursor.next_u8( op ) );
        (Op::Move( x, y ) , 1 + 1 + 1 )
      },
      0x22 => {
        let x = try!( cursor.next_u8( op ) );
        let v = try!( cursor.next_u8( op ) ) as u32;
        (Op::Set( x, v ) , 1 + 1 + 1 )
      },
      0x23 => {
        let o = try!( cursor.next_i16( op ) );
        (Op::Cursor( o ) , 1 + 2 )
      },
      _    => return Err( OpDecodeError::UnknownOp( op, code.len() - 1 ) )
    } )
  }

  // TODO: Don't panic, actually return an error
  fn execute( &self, task : &mut Task ) -> TaskStatus {

    // Higher bound index of the stack
    let high = if task.stack.len() == 0 {
      0
    } else {
      task.stack.len() - 1
    };

    /*match *self {
      Nop => {},
      Push( v ) => task.stack.push( v ),
      Pop =>
        match task.stack.pop() {
          None => panic!( "Tried to pop an empty stack" ),
          _ => {}
        },
      Dup => {
        assert!( !task.stack.is_empty() );

        let v = task.stack[high];
        task.stack.push( v );
      },
      Swap => {
        assert!( task.stack.len() >= 2 );

        let tmp = task.stack[high - 1];
        task.stack[high - 1] = task.stack[high];
        task.stack[high] = tmp;
      },
      Peek( n ) => {
        assert!( (n as usize) < task.stack.len() );

        let v = task.stack[high - (n as usize)];
        task.stack.push( v );
      },
      Rem( n ) => {
        assert!( (n as usize) < task.stack.len() );

        task.stack.remove( high - (n as usize) );
      },
      Yield => {
        return TaskStatus::Yielded
      },
      Call( f ) => {
        let code = task.constants.get_chunk( f as usize );

        task.code.code.push_all( code );
      },
      Callf( ff ) => {
        // Get and call the foreign function
        return task.constants.get::<ForeignFunction>( ff as usize )( task );
      },
      If( t, e ) => {
        assert!( !task.stack.is_empty() );
        let cond = task.stack.pop().unwrap();
        if cond != 0 {
          Call( t ).execute( task );
        } else {
          Call( e ).execute( task );
        }
      },
      Spawn( f ) => {
        let code = task.constants.get_code( f as usize );
        let ntask = Task::new( code, task.constants.clone() );
        return TaskStatus::Spawning( ntask )
      },
      Maptop( f ) => {
        assert!( !task.stack.is_empty() );

        let map = task.constants.get::<ForeignMap>( f as usize );
        map( &mut task.stack[high], task.constants.clone() );
      },
      Binop( f ) => {
        assert!( task.stack.len() >= 2 );

        let binop = task.constants.get::<ForeignBinop>( f as usize );
        let (b, a) = (task.stack[high], task.stack[high - 1]);
        task.stack[high - 1] = binop( a, b, task.constants.clone() );
        // Remove the last element
        task.stack.truncate( high );
      }
    }*/
    TaskStatus::Ok
  }
}

pub type ConstRef = usize;

// A tasks execution stack
pub type Stack = Vec<u32>;

// A code stack
#[derive(Clone)]
pub struct Code {
  code : Vec<u8>
}

impl Code {
  pub fn new( c : Vec<u8> ) -> Code {
    Code { code: c }
  }
}

impl Iterator for Code {
  type Item = Op;

  fn next( &mut self ) -> Option<Op> {
    // No need to continue
    if self.code.is_empty() { return None }

    match Op::decode( &self.code[..] ) {
      // The decoding succeeded with `bc` amount of bytes read
      Ok( (op, bc) ) => {
        // Remove the decoded bytes
        let new_length = self.code.len() - bc;
        self.code.truncate( new_length );
        Some( op )
      },
      // TODO: Handle this in a better way
      Err( err ) => panic!( "Error while decoding bytecode: {:?}", err )
    }
  }
}

pub enum TaskStatus {
  Ok,
  Dead,
  Yielded,
  Spawning( Task )
}

// A single unit of concurrent execution
pub struct Task {
  pub constants : Arc<ConstantTable>,
  pub stack : Stack,
  pub code  : Code,
}

impl Task {
  // TODO: Pre-allocate memory for stack and code-space
  pub fn new( code : Code, ct : Arc<ConstantTable> ) -> Task {
    Task { constants: ct
         , code     : code
         , stack    : Vec::new() }
  }

  pub fn step( &mut self ) -> TaskStatus {
    // TODO: Move this execution part to it's own
    match self.code.next() {
      Some( op ) => {
        let v = op.execute( self );
        v
      },
      None => TaskStatus::Dead
    }
  }
}

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

  pub fn get_code( &self, idx : usize ) -> Code {
    let code_bytes = self.get_chunk( idx );
    Code { code: code_bytes.to_vec() }
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
        TaskStatus::Yielded => self.next_task(),
        TaskStatus::Spawning( t ) => {
          self.outbox.send( WorkerReport::SpawnTask( t ) );
          self.tick_task()
        }
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
    self.scheduler.spawn_task( Task::new( self.constants.main()
                                        , self.constants.clone() ) );
    self.scheduler.run();
  }
}
