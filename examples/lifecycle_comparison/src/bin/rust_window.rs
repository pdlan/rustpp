use std::marker::PhantomPinned;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

trait Accessible {
    fn describe(&self) -> String;
}
trait EventTarget {
    fn event_kind(&self) -> &'static str;
    fn as_accessible(&self) -> &dyn Accessible;
}

struct Window {
    label: String,
    callbacks: Arc<Mutex<Vec<usize>>>,
    handle: Option<usize>,
    _pin: PhantomPinned,
}

impl Window {
    fn new(label: String, callbacks: Arc<Mutex<Vec<usize>>>) -> Pin<Box<Self>> {
        let mut owner = Box::pin(Self {
            label,
            callbacks,
            handle: None,
            _pin: PhantomPinned,
        });
        let userdata = &*owner as *const Self as usize;
        owner.callbacks.lock().unwrap().push(userdata);
        // SAFETY: publication happens only after pinning at the final address.
        unsafe { owner.as_mut().get_unchecked_mut().handle = Some(userdata) };
        owner
    }
}

impl Accessible for Window {
    fn describe(&self) -> String {
        format!("native window: {}", self.label)
    }
}
impl EventTarget for Window {
    fn event_kind(&self) -> &'static str {
        "window-event"
    }
    fn as_accessible(&self) -> &dyn Accessible {
        self
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        let userdata = self.handle.take().unwrap();
        self.callbacks
            .lock()
            .unwrap()
            .retain(|item| *item != userdata);
    }
}

fn main() {
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let target: Pin<Box<dyn EventTarget>> = Window::new("Settings".to_owned(), callbacks.clone());
    assert_eq!(target.event_kind(), "window-event");
    assert_eq!(target.as_accessible().describe(), "native window: Settings");
    drop(target);
    assert!(callbacks.lock().unwrap().is_empty());
    println!("window: callback installed, cross-cast, removed");
}
