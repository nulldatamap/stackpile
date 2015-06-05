#![feature(collections_drain, collections)]

extern crate num_cpus;

mod runtime;

use std::sync::Arc;
use runtime::{ RuntimeSettings
             , Runtime
             , Op
             , ConstantTable
             , Code
             , Task
             , TaskStatus };

fn ff_print( t : &mut Task ) -> TaskStatus {
  println!( "{}", t.stage[0] );
  TaskStatus::Ok
}

fn ff_less_than( a : u32, b : u32, _ : Arc<ConstantTable> ) -> u32 {
  if a < b { 1 } else { 0 }
}

fn ff_add( a : u32, b : u32, _ : Arc<ConstantTable> ) -> u32 {
  a + b
}

fn ff_sub( a : u32, b : u32, _ : Arc<ConstantTable> ) -> u32 {
  a - b
}

fn main() {
  use std::mem::transmute;
  println!( "Welcome to Stackpile!" );

  let P = 0xFF;
  let L = 0xFE;
  let A = 0xFD;
  let S = 0xFC;

  let mut f_main = vec![ 
      P, P, P, P, P, P, P, P             // 00 - print
    , L, L, L, L, L, L, L, L             // 08 - u32_lt
    , A, A, A, A, A, A, A, A             // 16 - u32_add
    , S, S, S, S, S, S, S, S             // 24 - u32_sub
    , 15, 0, 0, 0                         // 32 - main
    , 0, 0, 0, 0, 0, 0x05                //   call print
    , 51, 0, 0, 0, 0, 0x04               //   call 0 fib
    , 6, 0, 0x22                         //   set 0 3
    , 22, 0, 0, 0                        // 51 - fib
    , 84, 0, 0, 0, 77, 0, 0, 0, 1, 0x03  //   if 1 fib::then::0 fib::else::0
    , 8, 0, 0, 0, 1, 0x08                //   binop 1 u32_lt
    , 2, 2, 0x22                         //   set 2 2
    , 1, 0, 0x21                         //   move 0 1
    , 3, 0, 0, 0                         // 77 - fib::then::0
    , 1, 0, 0x22                         //   set 0 1
    , 45, 0, 0, 0                        // 84 - fib::else::0
    , 0, 1, 0x21                         //   move 1 0
    , 16, 0, 0, 0, 1, 0x08               //   binop 1 u32_add
    , 51, 0, 0, 0, 2, 0x04               //   call 2 fib
    , 24, 0, 0, 0, 2, 0x08               //   binop 2 u32_sub
    , 2, 3, 0x22                         //   set 3 2
    , 2, 0, 0x21                         //   move 0 2
    , 51, 0, 0, 0, 1, 0x04               //   call 1 fib
    , 24, 0, 0, 0, 1, 0x08               //   binop 1 u32_sub
    , 1, 2, 0x22                         //   set 2 1
    , 1, 0, 0x21                         //   move 0 1
  ] ;

  unsafe {
    let mut p : &mut (fn( &mut Task ) -> TaskStatus) =
      transmute( (&mut f_main[0] as *mut u8) );
    *p = ff_print;
    let mut o : &mut (fn( u32, u32, Arc<ConstantTable> ) -> u32) =
      transmute( (&mut f_main[8] as *mut u8) );
    *o = ff_less_than;
    o = transmute( (&mut f_main[16] as *mut u8) );
    *o = ff_add;
    o = transmute( (&mut f_main[24] as *mut u8) );
    *o = ff_sub;
  }
  
  let ct =
    ConstantTable { names: vec![ ("main".to_string(), 32) ]
                  , constants: f_main };

  let mut rt = Runtime::new( RuntimeSettings::default(), ct );
  rt.run();
}
