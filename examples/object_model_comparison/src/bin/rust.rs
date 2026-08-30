use std::any::Any;

trait Clickable {
    fn click(&mut self);
    fn click_count(&self) -> usize;
    fn complete_address(&self) -> usize;
}

trait Accessible {
    fn describe(&self) -> String;
    fn complete_address(&self) -> usize;
}

trait Widget: Any {
    fn paint(&self) -> String;
    fn complete_address(&self) -> usize;

    // Rust has no general sibling-trait cross-cast. Every discoverable facet
    // needs an explicit query hook on the erased interface.
    fn as_clickable_mut(&mut self) -> Option<&mut dyn Clickable>;
    fn as_accessible(&self) -> Option<&dyn Accessible>;

    // Owning concrete downcast also needs an explicit erasure escape hatch.
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

struct ToggleButton {
    // Rust has no concrete base subobjects, so the state of all three facets is
    // flattened or manually composed into the final struct.
    id: usize,
    clicks: usize,
    label: String,
    checked: bool,
}

impl ToggleButton {
    fn new(id: usize, label: String) -> Self {
        Self {
            id,
            clicks: 0,
            label,
            checked: false,
        }
    }

    fn is_checked(&self) -> bool {
        self.checked
    }
}

impl Clickable for ToggleButton {
    fn click(&mut self) {
        self.clicks += 1;
        self.checked = !self.checked;
    }

    fn click_count(&self) -> usize {
        self.clicks
    }

    fn complete_address(&self) -> usize {
        self as *const Self as usize
    }
}

impl Accessible for ToggleButton {
    fn describe(&self) -> String {
        let state = if self.checked { "checked" } else { "unchecked" };
        format!("{}: {state}", self.label)
    }

    fn complete_address(&self) -> usize {
        self as *const Self as usize
    }
}

impl Widget for ToggleButton {
    fn paint(&self) -> String {
        format!("[ #{} {} ]", self.id, Accessible::describe(self))
    }

    fn complete_address(&self) -> usize {
        self as *const Self as usize
    }

    fn as_clickable_mut(&mut self) -> Option<&mut dyn Clickable> {
        Some(self)
    }

    fn as_accessible(&self) -> Option<&dyn Accessible> {
        Some(self)
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

fn main() {
    let concrete = Box::new(ToggleButton::new(7, "Dark mode".to_owned()));
    let original_address = Widget::complete_address(&*concrete);

    // Trait-object coercion provides the initial upcast.
    let mut widget: Box<dyn Widget> = concrete;
    assert_eq!(widget.complete_address(), original_address);
    assert_eq!(widget.paint(), "[ #7 Dark mode: unchecked ]");

    // The cross-casts are handwritten query methods rather than a general
    // operation supported by the object model.
    {
        let clickable = widget.as_clickable_mut().expect("clickable facet");
        clickable.click();
        assert_eq!(clickable.click_count(), 1);
        assert_eq!(clickable.complete_address(), original_address);
    }

    let accessible = widget.as_accessible().expect("accessible facet");
    assert_eq!(accessible.describe(), "Dark mode: checked");
    assert_eq!(accessible.complete_address(), original_address);

    // Recovering the concrete owner requires Any plus a consuming hook on the
    // Widget trait. Adding another target trait would require another hook.
    let toggle = widget
        .into_any()
        .downcast::<ToggleButton>()
        .unwrap_or_else(|_| panic!("expected ToggleButton"));
    assert!(toggle.is_checked());
    assert_eq!(Widget::complete_address(&*toggle), original_address);

    println!("{}", toggle.paint());
    println!("clicks={}, same_address=true", toggle.click_count());
}
