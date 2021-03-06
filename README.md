[![Build Status](https://travis-ci.org/zutils/signals.svg?branch=master)](https://travis-ci.org/zutils/signals)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/zutils/signals/blob/master/LICENSE-MIT)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# signals
Current version: 0.0.5

The Signals crate is an asynchronous functional-reactive-like api.

## Installation

Add this to cargo.toml
```
[dependencies]
signals = "*"
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
    let signal = Signal::new_arc_mutex( |x: &u32| Ok(*x) );
    let listener = Signal::new_arc_mutex( |x: &u32| { assert_ne!(*x, 5); Ok(()) } ); //fail!

    // when signal is emitted, listener should execute.
    signal.lock().register_listener(&listener);

    // emit signal
    signal.lock().emit(&5);
}
```

## Complex Example

```rust
use signals::{Signal, Emitter};

fn main() {
    // create a bunch of signals. At the last signal, we should fail.
    // If we do not fail, the signals did not work.
    let root = Signal::new_arc_mutex( |x: &u32| Ok(x.to_string()) ); //convert x to string.
    let peek = Signal::new_arc_mutex( |x: &String| { println!("Peek: {}", x); Ok(()) } );
    let to_i32 = Signal::new_arc_mutex( |x: &String| Ok(x.parse::<i32>()?) ); //convert to integer
    let inc = Signal::new_arc_mutex( |x: &i32| Ok(x+1) ); //increment value
    let fail = Signal::new_arc_mutex( |x: &i32| { assert_ne!(*x, 8); Ok(()) } ); //fail!

    //connect all signals together.
    root.lock().register_listener(&peek); // snoop on the value! - because we can!
    root.lock().register_listener(&to_i32); // parse the string to an integer
    to_i32.lock().register_listener(&inc); //increment the value
    inc.lock().register_listener(&fail); //then finally fail if the value is 8!

    root.lock().emit(&7); //7 will increment to 8 and fail!
}
```

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
