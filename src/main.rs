#![feature(collections_drain, collections)]

extern crate num_cpus;

mod runtime;

use runtime::{ RuntimeSettings
             , Runtime
             , Op
             , ConstantTable
             , Function
             , Code
             , Task
             , TaskStatus };

fn ff_print( t : &mut Task ) -> TaskStatus {
  println!( "{}", t.stack.pop().unwrap() );
  TaskStatus::Ok
}

fn ff_less_than( t : &mut Task ) -> TaskStatus {
  let b = t.stack.pop().unwrap();
  let a = t.stack.pop().unwrap();
  t.stack.push( if a < b { 1 } else { 0 } );
  TaskStatus::Ok
}

fn ff_add( t : &mut Task ) -> TaskStatus {
  let b = t.stack.pop().unwrap();
  let a = t.stack.pop().unwrap();
  t.stack.push( a + b );
  TaskStatus::Ok
}

fn ff_sub( t : &mut Task ) -> TaskStatus {
  let b = t.stack.pop().unwrap();
  let a = t.stack.pop().unwrap();
  t.stack.push( a - b );
  TaskStatus::Ok
}

fn main() {
  use std::mem::transmute;
  println!( "Welcome to Stackpile!" );

  let P = 0xFF;
  let L = 0xFE;
  let A = 0xFD;
  let S = 0xFC;

  let mut f_main = vec![ 
      15, 0, 0, 0                    //   0  - 3     main
    , 100, 0, 0, 0, 0x09             //   4  - 8     
    , 19, 0, 0, 0, 0x08              //   9  - 13    
    , 6, 0, 0, 0, 0x01               //  14 - 18    
    , 20, 0, 0, 0                    //  19 - 22    fib
    , 53, 0, 0, 0, 43, 0, 0, 0, 0x0A //  23 - 31    
    , 108, 0, 0, 0, 0x09             //  32 - 36    
    , 2, 0, 0, 0, 0x01               //  37 - 41    
    , 0x03                           //  42         
    , 6, 0, 0, 0                     //  43 - 46    fib::then
    , 1, 0, 0, 0, 0x01               //  47 - 51    
    , 0x2                            //  52         
    , 43, 0, 0, 0                    //  53 - 56    fib::else
    , 1, 0, 0, 0, 0x06               //  57 - 61    
    , 116, 0, 0, 0, 0x09             //  62 - 66    
    , 19, 0, 0, 0, 0x08              //  67 - 71    
    , 124, 0, 0, 0, 0x09             //  72 - 76    
    , 1, 0, 0, 0, 0x01               //  77 - 81    
    , 0x04                           //  81         
    , 19, 0, 0, 0, 0x08              //  82 - 86    
    , 124, 0, 0, 0, 0x09             //  87 - 91    
    , 2, 0, 0, 0, 0x01               //  92 - 96    
    , 0x03                           //  97         
    , 0x03                           //  98         
    , P, P, P, P, P, P, P, P         //  99 - 107
    , L, L, L, L, L, L, L, L         // 108 - 115
    , A, A, A, A, A, A, A, A         // 116 - 123
    , S, S, S, S, S, S, S, S         // 124 - 131
  ] ;

  unsafe {
    let mut p : &mut (fn( &mut Task ) -> TaskStatus) =
      transmute( (&mut f_main[100] as *mut u8) );
    *p = ff_print;
    p = transmute( (&mut f_main[108] as *mut u8) );
    *p = ff_less_than;
    p = transmute( (&mut f_main[116] as *mut u8) );
    *p = ff_add;
    p = transmute( (&mut f_main[124] as *mut u8) );
    *p = ff_sub;
  }
  
  let ct =
    ConstantTable { names: vec![ ("main".to_string(), 0) ]
                  , constants: f_main };

  let mut rt = Runtime::new( RuntimeSettings::default(), ct );
  rt.run();
}
