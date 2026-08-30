use std::marker::PhantomPinned;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

trait EventHandler {
    fn on_ready(self: Pin<&mut Self>) -> usize;
}

struct Connection {
    registry: Arc<Mutex<Vec<usize>>>,
    registration: Option<usize>,
    events: usize,
    _pin: PhantomPinned,
}

impl Connection {
    fn new(registry: Arc<Mutex<Vec<usize>>>) -> Pin<Box<Self>> {
        let mut owner = Box::pin(Self {
            registry,
            registration: None,
            events: 0,
            _pin: PhantomPinned,
        });
        let address = &*owner as *const Self as usize;
        owner.registry.lock().unwrap().push(address);
        // SAFETY: the value is pinned before its retained address is published.
        unsafe { owner.as_mut().get_unchecked_mut().registration = Some(address) };
        owner
    }
}

impl EventHandler for Connection {
    fn on_ready(self: Pin<&mut Self>) -> usize {
        // SAFETY: modifying `events` does not move the pinned value.
        let this = unsafe { self.get_unchecked_mut() };
        this.events += 1;
        this.events
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let address = self.registration.take().unwrap();
        self.registry
            .lock()
            .unwrap()
            .retain(|item| *item != address);
    }
}

fn main() {
    let registry = Arc::new(Mutex::new(Vec::new()));
    let mut handler: Pin<Box<dyn EventHandler>> = Connection::new(registry.clone());
    assert_eq!(registry.lock().unwrap().len(), 1);
    assert_eq!(handler.as_mut().on_ready(), 1);
    drop(handler);
    assert!(registry.lock().unwrap().is_empty());
    println!("reactor: registered, dispatched, unregistered");
}
