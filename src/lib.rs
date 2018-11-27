//! The Signals crate is an asynchronous functional-reactive-like api.
//!
//! # Installation
//!
//! Add this to cargo.toml
//! ```text
//! [dependencies]
//! signals = "*"
//! ```
//!
//! Add this to your crate
//! ```text
//! extern crate signals; 
//! ```
//!
//! # Simple Example
//!
//! ```should_panic
//! use signals::{Signal, Emitter, AmEmitter};
//!
//! fn main() {
//!     // create a signal that will assert when emitted
//!     let signal = Signal::new_arc_mutex( |x: u32| Ok(x) );
//!     let listener = Signal::new_arc_mutex( |x: u32| { assert_ne!(x, 5); Ok(()) } ); //fail!
//!     
//!     // when signal is emitted, listener should execute.
//!     signal.lock().unwrap().register_listener(&listener);
//!
//!     // emit signal
//!     signal.lock().unwrap().emit(5);
//! }
//! ```
//!
//! # Complex Example
//!
//! ```should_panic
//! use signals::{Signal, Emitter};
//!
//! fn main() {
//!     // create a bunch of signals. At the last signal, we should fail. 
//!     // If we do not fail, the signals did not work.
//!     let root = Signal::new_arc_mutex( |x: u32| Ok(x.to_string()) ); //convert x to string.
//!     let peek = Signal::new_arc_mutex( |x: String| { println!("Peek: {}", x); Ok(()) } );
//!     let to_i32 = Signal::new_arc_mutex( |x: String| Ok(x.parse::<i32>()?) ); //convert to integer
//!     let inc = Signal::new_arc_mutex( |x: i32| Ok(x+1) ); //increment value
//!     let fail = Signal::new_arc_mutex( |x: i32| { assert_ne!(x, 8); Ok(()) } ); //fail!
//!     
//!     //connect all signals together.
//!     root.lock().unwrap().register_listener(&peek); // snoop on the value! - because we can!
//!     root.lock().unwrap().register_listener(&to_i32); // parse the string to an integer
//!     to_i32.lock().unwrap().register_listener(&inc); //increment the value
//!     inc.lock().unwrap().register_listener(&fail); //then finally fail if the value is 8!
//!     
//!     root.lock().unwrap().emit(7); //7 will increment to 8 and fail!
//! }
//! ```

extern crate failure;

//external imports
use failure::Error;

//standard imports
use std::marker::PhantomData;
use std::sync::{Arc, Weak, Mutex, MutexGuard};
use std::thread;

/// This is a polymorphic trait allowing multiple generic signals to be stored in a list.
pub trait Emitter: Send {
    type input;
    
    /// Start running the built in callback functionality with the signal. Pass on the values to children.
    fn emit(&mut self, Self::input);
}

// because Arc<Mutex<T>> is sloppy
#[derive(Clone)]
pub struct Am<T: Sized>(Arc<Mutex<T>>);

pub type Wm<T> = Weak<Mutex<T>>;
pub type AmEmitter<T> = Am<Emitter<input=T>>;
type WmEmitter<T> = Wm<Emitter<input=T>>;

impl<T: Sized> Am<T> {
    pub fn new(data: T) -> Self {
        Am(Arc::new(Mutex::new(data)))
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }

    pub fn clone(&self) -> Self {
        Am(self.0.clone())
    }
}

/// When creating a Signal, This trait represents the closure Fn allowed.
pub trait SigFn<I, O>: Fn(I) -> Result<O, Error> {}
impl<F, I, O> SigFn<I, O> for F where F: Fn(I) -> Result<O, Error> {}

impl<I,O,F> Emitter for Signal<I,O,F> 
    where F: 'static + SigFn<I, O> + Send + Sync,
          O: 'static + PartialEq + Send + Sync + Clone,
          I: 'static + Send + Sync
{
    type input = I;
    
    /// Run closure implemented for signal and pass on the results to children.
    fn emit(&mut self, data: Self::input) {
        let output = (self.func)(data);

        match output {
            Ok(output) => {
                // Exit out of loop if the output didn't change.
                if let Some(ref out) = self.output {
                    if *out == output {
                        return;
                    }
                }
                
                // There are no no errors, emit signals for all children.
                self.output = Some(output);
                self.emit_children();
            },
            Err(e) => println!("Error: {:?}", e),
        };
    }
}

/// Signals are the bread and butter of the crate.  A signal can trigger other signals whose input is the same output as the original signal. Signals support both threaded and non-threaded children.
pub struct Signal<I, O, F> 
    where F: SigFn<I, O> 
{
    input: PhantomData<I>,
    output: Option<O>,
    func: F,

    listeners: Vec< WmEmitter<O> >,
    threaded_listeners: Vec< WmEmitter<O> >,
}

impl<I,O,F> Signal<I,O,F> 
    where F: 'static + SigFn<I, O> + Send + Sync,
          O: 'static + Send + Sync + Clone,
          I: 'static + Send + Sync,
{
    fn new_signal(f: F) -> Signal<I, O, impl SigFn<I, O>> {
        Signal {
            input: PhantomData,
            output: None,
            func: move |i: _| f(i),
            listeners: Vec::new(),
            threaded_listeners: Vec::new(),
        }
    }

    /// Create a thread-safe parent signal. Note that the return function is Arc<Mutex<Signal<>>>
    pub fn new_arc_mutex(f: F) -> Am<Signal<I, O, impl SigFn<I, O>>> {
        Am::new(Signal::new_signal(f))
    }

    /// Upgrade all weak emitters, and call emit(...)
    fn emit_children(&mut self) {
        //emit instant listeners
        for signal in self.listeners.iter() {
            let output = self.output.clone();
            Self::emit_child(signal.clone(), output);
        }
        
        //emit threaded listeners
        for signal in self.threaded_listeners.iter() {
            let signal = signal.clone();
            let output = self.output.clone();
            thread::spawn( move || Self::emit_child(signal, output) );
        }
    }

    fn emit_child(signal: WmEmitter<O>, output: Option<O>) {
        if let Some(signal) = signal.upgrade() {
            if let Some(ref output) = output {
                let output = output.clone();
                if let Ok(mut signal) = signal.lock() {
                    signal.emit(output);
                }
            }
        }
    }

    /// This method is a helper for Signal::new(f) and register_listener(...)
    pub fn create_listener<Q, G>(&mut self, f: G) -> Am<Signal<O, Q, impl SigFn<O,Q>>>
        where G: 'static + SigFn<O,Q> + Send + Sync,
              Q: 'static + PartialEq + Send + Sync + Clone,
              O: 'static
    {
        let ret = Signal::new_arc_mutex(f);
        self.register_listener(&ret);
        ret
    }
    
    /// This method is a helper for Signal::new(f) and register_threaded_listener(...)
    pub fn create_threaded_listener<Q, G>(&mut self, f: G) -> Am<Signal<O, Q, impl SigFn<O,Q>>>
        where G: 'static + SigFn<O,Q> + Send + Sync,
              Q: 'static + PartialEq + Send + Sync + Clone,
              O: 'static
    {
        let ret = Signal::new_arc_mutex(f);
        self.register_threaded_listener(&ret);
        ret
    }
    
    /// Register a child listener that will execute in the same thread as Self.
    pub fn register_listener<E>(&mut self, strong: &Am<E>) 
        where E: 'static + Emitter<input=O>
    {
        let weak = Arc::downgrade(&strong.0);
        self.listeners.push(weak);
    }
    
    /// Register a child listener that will run on its own thread.
    //pub fn register_threaded_listener(&mut self, strong: &AmEmitter<O>) {
    pub fn register_threaded_listener<E>(&mut self, strong: &Am<E>) 
        where E: 'static + Emitter<input=O>
    {
        let weak = Arc::downgrade(&strong.0);
        self.threaded_listeners.push(weak);
    }

    /// Get the last result of this signal... if it exists
    pub fn get(&self) -> &Option<O> { &self.output }
}
