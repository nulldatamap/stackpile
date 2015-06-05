use std::sync::Arc;
use std::ops::{Index, IndexMut};
use std::mem::transmute;

use op::{Op};
use runtime::{ConstRef, ConstantTable};

pub struct Stage {
  inner : Vec<u32>,
  cursor : usize
}

impl Stage {
  pub fn new( prealloc : usize ) -> Stage {
    Stage { inner: Vec::with_capacity( prealloc )
          , cursor: 0 }
  }

  pub fn offset( &mut self, offset : isize ) {
    let nc = offset + (self.cursor as isize);
    assert!( nc >= 0 );
    self.cursor = nc as usize;
  }
}

impl Index<usize> for Stage {
  type Output = u32;

  fn index<'a>( &'a self, idx : usize ) -> &'a u32 {

    if (self.inner.len() == 0) || (self.cursor + idx >= self.inner.len()) {
      panic!( "Can't resize when not mutably borrowed!" );
    }

    &self.inner[self.cursor + idx]
  }
}

impl IndexMut<usize> for Stage {

  fn index_mut<'a>( &'a mut self, idx : usize ) -> &'a mut u32 {

    if (self.inner.len() == 0) || (self.cursor + idx >= self.inner.len()) {
      self.inner.resize( self.cursor + idx + 1, 0 );
    }

    &mut self.inner[self.cursor + idx]
  }
}


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
  pub stage : Stage,
  pub code  : Code,
}

impl Task {
  // TODO: Pre-allocate memory for stack and code-space
  pub fn new( code : Code, ct : Arc<ConstantTable> ) -> Task {
    Task { constants: ct
         , code     : code
         , stage    : Stage::new( 64 ) }
  }

  pub fn step( &mut self ) -> TaskStatus {
    // TODO: Move this execution part to it's own
    match self.code.next() {
      Some( op ) => {
        op.execute( self )
      },
      None => TaskStatus::Dead
    }
  }

  pub fn expand( &mut self, c : ConstRef ) {
    self.code.code.push_all( self.constants.get_chunk( c ) );
  }

  pub fn temporary_offset( &mut self, offset : usize ) {
    if offset == 0 {
      return
    }

    // Push a reverse offset op
    let ro : &[u8; 2];
    unsafe {
      ro = transmute( &((-(offset as isize)) as i16) as *const i16 );
    }
    self.code.code.push( ro[0] );
    self.code.code.push( ro[1] );
    self.code.code.push( 0x23 );

    self.stage.offset( offset as isize );
  }
}
