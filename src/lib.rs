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
//!     signal.lock().register_listener(&listener);
//!
//!     // emit signal
//!     signal.lock().emit(&5);
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
//!     let root = Signal::new_arc_mutex( |x: &u32| Ok(x.to_string()) ); //convert x to string.
//!     let peek = Signal::new_arc_mutex( |x: &String| { println!("Peek: {}", x); Ok(()) } );
//!     let to_i32 = Signal::new_arc_mutex( |x: &String| Ok(x.parse::<i32>()?) ); //convert to integer
//!     let inc = Signal::new_arc_mutex( |x: &i32| Ok(x+1) ); //increment value
//!     let fail = Signal::new_arc_mutex( |x: &i32| { assert_ne!(*x, 8); Ok(()) } ); //fail!
//!     
//!     //connect all signals together.
//!     root.lock().register_listener(&peek); // snoop on the value! - because we can!
//!     root.lock().register_listener(&to_i32); // parse the string to an integer
//!     to_i32.lock().register_listener(&inc); //increment the value
//!     inc.lock().register_listener(&fail); //then finally fail if the value is 8!
//!     
//!     root.lock().emit(&7); //7 will increment to 8 and fail!
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
    
    /// Trigger the signal's function.
    fn emit(&mut self, &Self::input);
}

/// This is a polymorphic trait allowing any signals to be called without any input
pub trait EmitterNoInput: Send {
    /// Trigger the signal's function.
    fn emit(&mut self);
}

// because Arc<Mutex<T>> is sloppy
#[derive(Clone)]
pub struct Am<T: ?Sized>(pub Arc<Mutex<T>>);

pub type Wm<T> = Weak<Mutex<T>>;
type WmEmitter<T> = Wm<Emitter<input=T>>;
type WmEmitterNoInput = Wm<EmitterNoInput>;

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

impl<I,O> EmitterNoInput for Signal<I,O>   
    where O: 'static + Send + Sync + Clone + PartialEq,
          I: 'static + Send + Sync
{
    /// Run closure implemented for signal and pass on the results to children.
    fn emit(&mut self) {
        match (self.no_input_func)() {
            Ok(output) => self.emit_children_on_changed_output(output),
            Err(e) => println!("Error: {:?}", e),
        }
    }
}

impl<I,O> Emitter for Signal<I,O> 
    where O: 'static + Send + Sync + Clone + PartialEq,
          I: 'static + Send + Sync
{
    type input = I;
    
    /// Run closure implemented for signal and pass on the results to children.
    fn emit(&mut self, data: &Self::input) {
        match (self.func)(data) {
            Ok(output) => self.emit_children_on_changed_output(output),
            Err(e) => println!("Error: {:?}", e),
        }
    }
}

/// Signals are the bread and butter of the crate.  A signal can trigger other signals whose input is the same output as the original signal. Signals support both threaded and non-threaded children.
pub struct Signal<I, O> {
    input: PhantomData<I>,
    output: Option<O>,
    func: Box<FnMut(&I) -> Result<O, Error> + Send + Sync>,
    no_input_func: Box<FnMut() -> Result<O, Error> + Send + Sync>,
    dirty_flag: bool,

    listeners: Vec< WmEmitter<O> >,
    listeners_no_input: Vec< WmEmitterNoInput >,
    threaded_listeners: Vec< WmEmitter<O> >,
}

impl<I,O> Signal<I,O> 
    where O: 'static + Send + Sync + Clone + PartialEq,
          I: 'static + Send + Sync,
{
    fn new<F>(f: F) -> Self
        where F: 'static + FnMut(&I) -> Result<O, Error> + Send + Sync,
    {
        Signal {
            input: PhantomData,
            output: None,
            func: Box::new(f),
            no_input_func: Box::new(|| Err(failure::format_err!("no_input_func is empty!")) ),
            dirty_flag: true,
            listeners: Vec::new(),
            listeners_no_input: Vec::new(),
            threaded_listeners: Vec::new(),
        }
    }

    fn new_no_input<F>(f: F) -> Self
        where F: 'static + FnMut() -> Result<O, Error> + Send + Sync,
    {
        Signal {
            input: PhantomData,
            output: None,
            func: Box::new(|_| Err(failure::format_err!("func is empty!")) ),
            no_input_func: Box::new(f),
            dirty_flag: true,
            listeners: Vec::new(),
            listeners_no_input: Vec::new(),
            threaded_listeners: Vec::new(),
        }
    }

    /// Has this signal been called since the last reset_dirty_flag()?
    pub fn take_dirty_flag(&mut self) -> bool {
        let ret = self.dirty_flag;
        self.dirty_flag = false;
        ret
    }

    pub fn emit_children_on_changed_output(&mut self, output: O) {
        if let Some(ref out) = self.output {
            if *out == output {
                return;
            }
        }

        self.output = Some(output);
        self.dirty_flag = true;
        self.emit_children();
    }

    /// Create a thread-safe parent signal. Note that the return function is Arc<Mutex<Signal<>>>
    pub fn new_arc_mutex<F>(f: F) -> Am<Signal<I, O>> 
        where F: 'static + FnMut(&I) -> Result<O, Error> + Send + Sync,
    {
        Am::new(Signal::new(f))
    }

    /// Create a thread-safe parent signal. Note that the return function is Arc<Mutex<Signal<>>>
    pub fn new_arc_mutex_no_input<F>(f: F) -> Am<Signal<I, O>> 
        where F: 'static + FnMut() -> Result<O, Error> + Send + Sync,
    {
        Am::new(Signal::new_no_input(f))
    }

    /// Upgrade all weak emitters, and call emit(...)
    fn emit_children(&mut self) {
        //emit instant listeners
        for signal in self.listeners.iter() {
            Self::emit_child(signal.clone(), self.output.as_ref());
        }

        //emit listeners that take no input
        for signal in self.listeners_no_input.iter() {
            Self::emit_child_no_input(signal.clone());
        }
        
        //emit threaded listeners
        for signal in self.threaded_listeners.iter() {
            let signal = signal.clone();
            let output = self.output.clone();
            thread::spawn( move || Self::emit_child(signal, output.as_ref()) );
        }
    }

    fn emit_child(signal: WmEmitter<O>, output: Option<&O>) {
        if let Some(signal) = signal.upgrade() {
            if let Some(ref output) = output {
                if let Ok(mut signal) = signal.lock() {
                    signal.emit(output);
                }
            }
        }
    }

    fn emit_child_no_input(signal: WmEmitterNoInput) {
        if let Some(signal) = signal.upgrade() {
            if let Ok(mut signal) = signal.lock() {
                signal.emit();
            }
        }
    }

    /// This method is a helper for Signal::new(f) and register_listener(...)
    pub fn create_listener<Q, G, F>(&mut self, f: F) -> Am<Signal<O, Q>>
        where F: 'static + FnMut(&O) -> Result<Q, Error> + Send + Sync,
              Q: 'static + PartialEq + Send + Sync + Clone,
              O: 'static
    {
        let ret = Signal::new_arc_mutex(f);
        self.register_listener(&ret);
        ret
    }
    
    /// This method is a helper for Signal::new(f) and register_threaded_listener(...)
    pub fn create_threaded_listener<Q, G, F>(&mut self, f: F) -> Am<Signal<O, Q>>
        where F: 'static + FnMut(&O) -> Result<Q, Error> + Send + Sync,
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

    /// Register a child listener that will execute in the same thread as Self.
    pub fn register_listener_no_input<E>(&mut self, strong: &Am<E>) 
        where E: 'static + EmitterNoInput
    {
        let weak = Arc::downgrade(&strong.0);
        self.listeners_no_input.push(weak);
    }
    
    /// Register a child listener that will run on its own thread.
    pub fn register_threaded_listener<E>(&mut self, strong: &Am<E>) 
        where E: 'static + Emitter<input=O>
    {
        let weak = Arc::downgrade(&strong.0);
        self.threaded_listeners.push(weak);
    }

    /// Get the last result of this signal... if it exists
    pub fn get(&self) -> &Option<O> { &self.output }
}
