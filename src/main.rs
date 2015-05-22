extern crate num_cpus;

mod runtime;

use runtime::{ RuntimeSettings
             , Runtime
             , Op
             , ConstVal
             , ConstantTable
             , TaggedValue
             , Function };

fn main() {
  println!( "Welcome to Stackpile!" );

  // main -> print 13.37 42
  let f_main =
      Function { arity: 0
               , code : vec![ Op::Push( TaggedValue::Int( 42 ) )
                            , Op::Push( TaggedValue::Float( 13.37 ) )
                            , Op::PrintStack ] };

  let ct =
    ConstantTable { names: vec![ ("main".to_string(), 0) ]
                  , constants: vec![ ConstVal::Function( f_main ) ] };

  let mut rt = Runtime::new( RuntimeSettings::default(), ct );
  rt.run();
}
