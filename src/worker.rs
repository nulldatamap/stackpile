use std::sync::mpsc::{TryRecvError, Sender, Receiver};
use std::thread;

use task::{Task, TaskStatus};

pub type WorkerId = u32;

// A reference to a given worker
pub struct WorkerHandle {
  pub id         : WorkerId,
  pub outbox     : Sender<WorkerCommand>,
  pub task_count : u32,
  pub handle     : thread::JoinHandle<()>
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
  pub fn new( id : WorkerId, inbox : Receiver<WorkerCommand>
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

  // Start's the worker's main loop
  pub fn run( &mut self ) {
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

  // Switches to the next task, or waits for a task to be assigned
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

  // Wait's for a task to be assigned
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

  // Executes a single step of the current task
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

  // Remove a dead task and assigns a new one
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
