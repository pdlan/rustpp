use std::marker::PhantomPinned;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

trait Cancellable {
    fn cancel(self: Pin<&mut Self>);
    fn is_cancelled(&self) -> bool;
}
trait Runnable {
    fn run(self: Pin<&mut Self>) -> usize;
    fn as_cancellable(self: Pin<&mut Self>) -> Pin<&mut dyn Cancellable>;
}

struct Task {
    members: Arc<Mutex<Vec<usize>>>,
    link: Option<usize>,
    runs: usize,
    cancelled: bool,
    _pin: PhantomPinned,
}

impl Task {
    fn new(members: Arc<Mutex<Vec<usize>>>) -> Pin<Box<Self>> {
        let mut owner = Box::pin(Self {
            members,
            link: None,
            runs: 0,
            cancelled: false,
            _pin: PhantomPinned,
        });
        let address = &*owner as *const Self as usize;
        owner.members.lock().unwrap().push(address);
        // SAFETY: intrusive membership is installed only after pinning.
        unsafe { owner.as_mut().get_unchecked_mut().link = Some(address) };
        owner
    }
}

impl Cancellable for Task {
    fn cancel(self: Pin<&mut Self>) {
        // SAFETY: changing a flag does not move the task or its intrusive link.
        unsafe { self.get_unchecked_mut().cancelled = true };
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl Runnable for Task {
    fn run(self: Pin<&mut Self>) -> usize {
        // SAFETY: changing a counter does not move the pinned task.
        let this = unsafe { self.get_unchecked_mut() };
        this.runs += 1;
        this.runs
    }
    fn as_cancellable(self: Pin<&mut Self>) -> Pin<&mut dyn Cancellable> {
        self
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        let address = self.link.take().unwrap();
        self.members.lock().unwrap().retain(|item| *item != address);
    }
}

fn main() {
    let members = Arc::new(Mutex::new(Vec::new()));
    let mut runnable: Pin<Box<dyn Runnable>> = Task::new(members.clone());
    assert_eq!(runnable.as_mut().run(), 1);
    let mut cancellable = runnable.as_mut().as_cancellable();
    cancellable.as_mut().cancel();
    assert!(cancellable.is_cancelled());
    drop(runnable);
    assert!(members.lock().unwrap().is_empty());
    println!("scheduler: linked, cross-cast, unlinked");
}
