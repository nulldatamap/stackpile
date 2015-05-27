#![feature(collections_drain, collections)]

extern crate num_cpus;

mod runtime;

use runtime::{ RuntimeSettings
             , Runtime
             , Op
             , ConstantTable
             , Function
             , Code };

fn main() {
  println!( "Welcome to Stackpile!" );

  let f_main = vec![ 
      39 // 39 byte chunk
    , 0
    , 0
    , 0
    , 0x00 // NOP
    , 1, 0, 0, 0, 0x01 // PUSH 1
    , 0x02 // POP
    , 0x03 // DUP     
    , 0x04 // SWAP   
    , 1, 0, 0, 0, 0x05 // PUT 1
    , 1, 0, 0, 0, 0x06 // REM 1 
    , 0x07 // YIELD  
    , 1, 0, 0, 0, 0x08 // CALL 1
    , 1, 0, 0, 0, 0x09 // CALLF 1
    , 2, 0, 0, 0, 1, 0, 0, 0, 0x0A // IF 1 2 
  ] ;

  let ct =
    ConstantTable { names: vec![ ("main".to_string(), 0) ]
                  , constants: f_main };

  let mut rt = Runtime::new( RuntimeSettings::default(), ct );
  rt.run();
}
