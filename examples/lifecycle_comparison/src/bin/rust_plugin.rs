use std::marker::PhantomPinned;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

trait RequestHandler {
    fn handle(&self) -> &'static str;
}
trait MetricsSource {
    fn metric(&self) -> usize;
}
trait Plugin {
    fn name(&self) -> &'static str;
    fn as_request_handler(&self) -> &dyn RequestHandler;
    fn as_metrics_source(&self) -> &dyn MetricsSource;
}

struct HttpPlugin {
    published: Arc<Mutex<Vec<usize>>>,
    token: Option<usize>,
    _pin: PhantomPinned,
}

impl HttpPlugin {
    fn new(published: Arc<Mutex<Vec<usize>>>) -> Pin<Box<Self>> {
        let mut owner = Box::pin(Self {
            published,
            token: None,
            _pin: PhantomPinned,
        });
        let address = &*owner as *const Self as usize;
        owner.published.lock().unwrap().push(address);
        // SAFETY: all interfaces are published only after final pinning.
        unsafe { owner.as_mut().get_unchecked_mut().token = Some(address) };
        owner
    }
}

impl RequestHandler for HttpPlugin {
    fn handle(&self) -> &'static str {
        "200 OK"
    }
}
impl MetricsSource for HttpPlugin {
    fn metric(&self) -> usize {
        1
    }
}
impl Plugin for HttpPlugin {
    fn name(&self) -> &'static str {
        "http"
    }
    fn as_request_handler(&self) -> &dyn RequestHandler {
        self
    }
    fn as_metrics_source(&self) -> &dyn MetricsSource {
        self
    }
}
impl Drop for HttpPlugin {
    fn drop(&mut self) {
        let address = self.token.take().unwrap();
        self.published
            .lock()
            .unwrap()
            .retain(|item| *item != address);
    }
}

fn main() {
    let published = Arc::new(Mutex::new(Vec::new()));
    let plugin: Pin<Box<dyn Plugin>> = HttpPlugin::new(published.clone());
    assert_eq!(plugin.name(), "http");
    assert_eq!(plugin.as_request_handler().handle(), "200 OK");
    assert_eq!(plugin.as_metrics_source().metric(), 1);
    drop(plugin);
    assert!(published.lock().unwrap().is_empty());
    println!("plugin: published three views, queried, unpublished");
}
