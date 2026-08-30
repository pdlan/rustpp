use std::any::Any;

trait Widget: Any {
    fn paint(&self) -> String;
    fn click(&mut self);
    fn describe(&self) -> String;
    fn click_count(&self) -> usize;
    fn complete_address(&self) -> usize;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

struct ToggleButton {
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
}

impl Widget for ToggleButton {
    fn paint(&self) -> String {
        format!("[ #{} {} ]", self.id, self.describe())
    }

    fn click(&mut self) {
        self.clicks += 1;
        self.checked = !self.checked;
    }

    fn describe(&self) -> String {
        let state = if self.checked { "checked" } else { "unchecked" };
        format!("{}: {state}", self.label)
    }

    fn click_count(&self) -> usize {
        self.clicks
    }

    fn complete_address(&self) -> usize {
        self as *const Self as usize
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

fn main() {
    let concrete = Box::new(ToggleButton::new(7, "Dark mode".to_owned()));
    let original_address = concrete.complete_address();
    let mut widget: Box<dyn Widget> = concrete;

    assert_eq!(widget.complete_address(), original_address);
    assert_eq!(widget.paint(), "[ #7 Dark mode: unchecked ]");

    // Calls are simple, but Clickable and Accessible are no longer independent
    // facets: every Widget implementation shares this central interface.
    widget.click();
    assert_eq!(widget.describe(), "Dark mode: checked");

    let toggle = widget
        .into_any()
        .downcast::<ToggleButton>()
        .unwrap_or_else(|_| panic!("expected ToggleButton"));
    assert_eq!(toggle.complete_address(), original_address);

    println!("{}", toggle.paint());
    println!("clicks={}, same_address=true", toggle.click_count());
}
