use std::sync::Arc;
use std::mem::transmute;

use task::{TaskStatus, Task};
use runtime::{ConstRef, ConstantTable};

// The function types of different kinds of foreign functions
type ForeignFunction = fn( &mut Task ) -> TaskStatus;
type ForeignMap      = fn( u32, Arc<ConstantTable> ) -> u32;
type ForeignBinop    = fn( u32, u32, Arc<ConstantTable> ) -> u32;

#[derive(Debug, Clone)]
pub enum OpDecodeError {
  UnknownOp( u8, usize ),
  NoArgument( u8, usize )
}

// An operation that can be executed, spec in `opcodes.md`
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

// A reader's cursor for decoding op-codes
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
  // Tries to decode an instruction
  pub fn decode( code : &[u8] ) -> Result<Op, OpDecodeError> {
    let mut cursor = OpCursor { index: code.len() - 1, code: code };
    let op = code[code.len() - 1];

    Ok( match op {
      0x00 => Op::Nop,
      0x01 => Op::Yield,
      0x02 => {
        let val = try!( cursor.next_u32( op ) );
        Op::Spawn( val )
      },
      0x03 => {
        let x = try!( cursor.next_u8( op ) );
        let t = try!( cursor.next_u32( op ) );
        let e = try!( cursor.next_u32( op ) );
        Op::If( x, t, e )
      },
      0x04 => {
        let x = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        Op::Call( x, f )
      },
      0x05 => {
        let x = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        Op::Callf( x, f )
      },
      0x06 => {
        let x = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        Op::Map( x, f )
      },
      0x07 => {
        let x = try!( cursor.next_u8( op ) );
        let y = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        Op::Mapto( x, y, f )
      },
      0x08 => {
        let x = try!( cursor.next_u8( op ) );
        let f = try!( cursor.next_u32( op ) );
        Op::Binop( x, f )
      },
      0x09 => {
        let x = try!( cursor.next_u8( op ) );
        let v = try!( cursor.next_u8( op ) );
        Op::Callv( x, v )
      },
      0x20 => {
        let x = try!( cursor.next_u8( op ) );
        let y = try!( cursor.next_u8( op ) );
        Op::Swap( x, y )
      },
      0x21 => {
        let x = try!( cursor.next_u8( op ) );
        let y = try!( cursor.next_u8( op ) );
        Op::Move( x, y )
      },
      0x22 => {
        let x = try!( cursor.next_u8( op ) );
        let v = try!( cursor.next_u8( op ) ) as u32;
        Op::Set( x, v )
      },
      0x23 => {
        let o = try!( cursor.next_i16( op ) );
        Op::Cursor( o )
      },
      _    => return Err( OpDecodeError::UnknownOp( op, code.len() - 1 ) )
    } )
  }

  // Returns how wide each instruction is in bytes ( in it's encoded form )
  pub fn byte_width( &self ) -> usize {
    use self::Op::*;

    match *self {
      Nop | Yield => 1,
      Callv( .. ) | Swap( .. ) | Move( .. ) | Set( .. ) | Cursor( .. ) => 3,
      Spawn( .. ) => 5,
      Call( .. ) | Callf( .. ) | Map( .. ) | Binop( .. ) => 6,
      Mapto( .. ) => 7,
      If( .. ) => 10,
    }
  }

  // TODO: Don't panic, actually return an error
  // Executes an instruction in a given task
  pub fn execute( &self, task : &mut Task ) -> TaskStatus {
    use self::Op::*;

    match *self {
      Nop => {},
      Yield => {
        return TaskStatus::Yielded
      },
      Spawn( f ) => {
        let t =
          Task::new( task.constants.get_code( f ), task.constants.clone() );
        return TaskStatus::Spawning( t );
      },
      If( x, t, e ) => {
        let b = if task.stage[x] != 0 { t } else { e };
        task.expand( b );
      },
      Call( x, f ) => {
        // Make sure we have the stage setup correctly for the call
        task.temporary_offset( x );

        task.expand( f );
      },
      Callf( x, f ) => {
        task.temporary_offset( x );
        
        return task.constants.get::<ForeignFunction>( f )( task );
      },
      Map( x, f ) => {
        let map = task.constants.get::<ForeignMap>( f );
        task.stage[x] = map( task.stage[x], task.constants.clone() );
      },
      Mapto( x, y, f ) => {
        let map = task.constants.get::<ForeignMap>( f );
        task.stage[y] = map( task.stage[x], task.constants.clone() );
      },
      Binop( x, f ) => {
        let binop = task.constants.get::<ForeignBinop>( f );
        task.stage[x] =
          binop( task.stage[x], task.stage[x + 1], task.constants.clone() );
      },
      Callv( x, v ) => {
        Callf( x, task.stage[v] as ConstRef ).execute( task );
      },
      Swap( x, y ) => {
        let tmp = task.stage[x];
        task.stage[x] = task.stage[y];
        task.stage[y] = tmp;
      },
      Move( x, y ) => {
        task.stage[y] = task.stage[x];
      },
      Set( x, v ) => {
        task.stage[x] = v;
      },
      Cursor( o ) => {
        task.stage.offset( o );
      }
    }
    TaskStatus::Ok
  }
}
