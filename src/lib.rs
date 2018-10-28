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
//! use signals::{Signal, Emitter};
//!
//! fn main() {
//!     // create a signal that will assert when emitted
//!     let signal = Signal::new_arc_mutex( |x: &u32| Ok(*x) );
//!     let listener = Signal::new_arc_mutex( |x: &u32| { assert_ne!(*x, 5); Ok(()) } ); //fail!
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
//!     let root = Signal::new_arc_mutex( |x: &u32| Ok((*x).to_string()) ); //convert x to string.
//!     let peek = Signal::new_arc_mutex( |x: &String| { println!("Peek: {}", x); Ok(()) } );
//!     let to_i32 = Signal::new_arc_mutex( |x: &String| Ok(x.parse::<i32>()?) ); //convert to integer
//!     let inc = Signal::new_arc_mutex( |x: &i32| Ok(*x+1) ); //increment value
//!     let fail = Signal::new_arc_mutex( |x: &i32| { assert_ne!(*x, 8); Ok(()) } ); //fail!
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
use std::sync::{Arc, Weak, Mutex};
use std::thread;

/// This is a polymorphic trait allowing multiple generic signals to be stored in a list.
pub trait Emitter: Send {
    type input;
    
    /// When dealing with multithreaded Emitters, we want Arc capability
    fn emit_arc(&mut self, Arc<Self::input>);
    
    /// This is syntax sugar used as a emit_arc(...) wrapper
    fn emit(&mut self, Self::input);
}

// because Arc<Mutex<T>> is sloppy
pub type Am<T> = Arc<Mutex<T>>;
pub type Wm<T> = Weak<Mutex<T>>;

/// When creating a Signal, This trait represents the closure Fn allowed.
pub trait SigFn<I, O>: Fn(&I) -> Result<O, Error> {}
impl<F, I, O> SigFn<I, O> for F where F: Fn(&I) -> Result<O, Error> {}

impl<I,O,F> Emitter for Signal<I,O,F> 
    where F: SigFn<I, O> + Send,
          O: 'static + PartialEq + Send + Sync,
          I: Send
{
    type input = I;
    
    /// This emit is a helper function. It will copy the input into an Arc<>
    fn emit(&mut self, data: Self::input) {
        self.emit_arc(Arc::new(data));
    }
    
    /// This emit takes an Arc,  and will emit the calculated value to children when the calculated value changes.
    fn emit_arc(&mut self, data: Arc<Self::input> ) {
        let data_copy = data.clone();
        let output = self.arc_wrapper_func(&data_copy); 
        
        match output {
            // if there were no errors, emit signals for all children.
            Ok(output) => {
                // emit children if the output changes
                if *self.output != *output {
                    self.output = output;
                    self.emit_children();
                }
            },
            // if there were errors, then print them out.
            Err(e) => println!("Error: {:?}", e),
        };
    }
}

/// Signals are the bread and butter of the crate.  A signal can trigger other signals whose input is the same output as the original signal. Signals support both threaded and non-threaded children.
pub struct Signal<I, O, F> 
    where F: SigFn<I, O> 
{
    input: PhantomData<I>,
    output: Arc<O>,
    func: F,

    listeners: Vec< Wm<Emitter<input=O>> >,
    threaded_listeners: Vec< Wm<Emitter<input=O>> >,
}

impl<I,O,F> Signal<I,O,F> 
    where F: SigFn<I, O>,
          O: 'static + Send + Sync
{
    fn new_signal(f: F) -> Signal<I, O, impl SigFn<I, O>>
        where F: SigFn<I, O>,
              O: Default
    {
        Signal {
            input: PhantomData,
            output: Default::default(),
            func: move |i: &_| f(i),
            listeners: Vec::new(),
            threaded_listeners: Vec::new(),
        }
    }

    /// Create a thread-safe parent signal. Note that the return function is Arc<Mutex<Signal<>>>
    pub fn new_arc_mutex(f: F) -> Am<Signal<I, O, impl SigFn<I, O>>>
        where F: SigFn<I, O>,
              O: Default
    {
        Arc::new(Mutex::new(Signal::new_signal(f)))
    }
    
    /// Private function whose purpose is to allow Signal data to use threadsafe data, all while having simple looking closures.
    /// The wife refers to this as "Our Crapper"
    fn arc_wrapper_func(&self, input: &Arc<I>) -> Result<Arc<O>, Error> {
        Ok(Arc::new((self.func)(&input)?))
    }
    
    /// Upgrade all weak emitters, and call emit(...)
    fn emit_children(&mut self) {
        //emit instant listeners
        for signal in &mut self.listeners {
            if let Some(signal) = signal.upgrade() {
                let output_copy = self.output.clone();
                if let Ok(mut signal) = signal.lock() {
                    signal.emit_arc(output_copy);
                }
            }
        }
        
        //emit threaded listeners
        for signal in &mut self.threaded_listeners {
            let weak_sig = signal.clone();
            let output_copy = self.output.clone(); //copy output to send to listeners.
            
            //create a new thread. In the thread, upgrade the weak signal, lock it
            thread::spawn( move || { 
                if let Some(signal) = weak_sig.upgrade() {
                    if let Ok(mut signal) = signal.lock() {
                        signal.emit_arc(output_copy);
                    }
                }
            });
        }
    }

    /// This method is a helper for Signal::new(f) and register_listener(...)
    pub fn create_listener<Q, G>(&mut self, f: G) -> Arc<Mutex<Signal<O, Q, impl SigFn<O,Q>>>> 
        where G: 'static + SigFn<O,Q> + Send,
              Q: 'static + Default + PartialEq + Send + Sync,
              O: 'static
    {
        let ret = Signal::new_arc_mutex(f);
        self.register_listener(&ret);
        ret
    }
    
    /// This method is a helper for Signal::new(f) and register_threaded_listener(...)
    pub fn create_threaded_listener<Q, G>(&mut self, f: G) -> Arc<Mutex<Signal<O, Q, impl SigFn<O,Q>>>> 
        where G: 'static + SigFn<O,Q> + Send,
              Q: 'static + Default + PartialEq + Send + Sync,
              O: 'static
    {
        let ret = Signal::new_arc_mutex(f);
        self.register_threaded_listener(&ret);
        ret
    }
    
    /// Register a child listener that will execute in the same thread as Self.
    pub fn register_listener<S>(&mut self, strong: &Arc<Mutex<S>>) 
        where S: 'static + Emitter<input=O>
    {
        let weak = Arc::downgrade(&strong);
        self.listeners.push(weak);
    }
    
    /// Register a child listener that will run on its own thread.
    pub fn register_threaded_listener<S>(&mut self, strong: &Arc<Mutex<S>>) 
        where S: 'static + Emitter<input=O>
    {
        let weak = Arc::downgrade(&strong);
        self.threaded_listeners.push(weak);
    }

    /// Get the last result of this signal. If signal has not run, then it will return the Signal's output's default value.
    pub fn get(&self) -> Arc<O> { self.output.clone() }
}


/*#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}*/
