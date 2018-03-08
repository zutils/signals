[![Build Status](https://travis-ci.org/zutils/signals.svg?branch=master)](https://travis-ci.org/zutils/signals)
[![license mit](https://img.shields.io/github/license/mashape/apistatus.svg)](https://github.com/zutils/signals/blob/master/LICENSE-MIT)
[![license apache](https://img.shields.io/github/license/mashape/apistatus.svg)](https://github.com/zutils/signals/blob/master/LICENSE-APACHE)



# signals

The Signals crate is an asynchronous functional-reactive-like api.

## Installation

Add this to cargo.toml
```
[dependencies]
signals = "0.0.1"
```

Add this to your crate
```
extern crate signals;
```

## Simple Example

```rust
use signals::{Signal, Emitter};

fn main() {
    // create a signal that will assert when emitted
    let signal = Signal::new( |x: &u32| Ok(*x) );
    let listener = Signal::new( |x: &u32| { assert_ne!(*x, 5); Ok(()) } ); //fail!

    // when signal is emitted, listener should execute.
    signal.lock().unwrap().register_listener(&listener);

    // emit signal
    signal.lock().unwrap().emit(5);
}
```

## Complex Example

```rust
use signals::{Signal, Emitter};

fn main() {
    // create a bunch of signals. At the last signal, we should fail.
    // If we do not fail, the signals did not work.
    let root = Signal::new( |x: &u32| Ok((*x).to_string()) ); //convert x to string.
    let peek = Signal::new( |x: &String| { println!("Peek: {}", x); Ok(()) } );
    let to_i32 = Signal::new( |x: &String| Ok(x.parse::<i32>()?) ); //convert to integer
    let inc = Signal::new( |x: &i32| Ok(*x+1) ); //increment value
    let fail = Signal::new( |x: &i32| { assert_ne!(*x, 8); Ok(()) } ); //fail!

    //connect all signals together.
    root.lock().unwrap().register_listener(&peek); // snoop on the value! - because we can!
    root.lock().unwrap().register_listener(&to_i32); // parse the string to an integer
    to_i32.lock().unwrap().register_listener(&inc); //increment the value
    inc.lock().unwrap().register_listener(&fail); //then finally fail if the value is 8!

    root.lock().unwrap().emit(7); //7 will increment to 8 and fail!
}
```

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
