enum WidgetNode {
    ToggleButton(Box<ToggleButton>),
    #[allow(dead_code)]
    Label(Box<Label>),
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

    fn paint(&self) -> String {
        let _id = self.id;
        format!("[ {} ]", self.describe())
    }

    fn click(&mut self) {
        self.clicks += 1;
        self.checked = !self.checked;
    }

    fn describe(&self) -> String {
        let state = if self.checked { "checked" } else { "unchecked" };
        format!("{}: {state}", self.label)
    }
}

#[allow(dead_code)]
struct Label {
    text: String,
}

impl WidgetNode {
    fn paint(&self) -> String {
        match self {
            Self::ToggleButton(toggle) => toggle.paint(),
            Self::Label(label) => label.text.clone(),
        }
    }

    fn complete_address(&self) -> usize {
        match self {
            Self::ToggleButton(toggle) => &**toggle as *const ToggleButton as usize,
            Self::Label(label) => &**label as *const Label as usize,
        }
    }
}

fn main() {
    let concrete = Box::new(ToggleButton::new(7, "Dark mode".to_owned()));
    let original_address = &*concrete as *const ToggleButton as usize;
    // The inner Box preserves the concrete address when the enum is consumed.
    let mut widget = Box::new(WidgetNode::ToggleButton(concrete));

    assert_eq!(widget.complete_address(), original_address);
    assert_eq!(widget.paint(), "[ Dark mode: unchecked ]");

    // Every capability operation is an exhaustive variant match.
    let WidgetNode::ToggleButton(toggle) = &mut *widget else {
        panic!("expected ToggleButton")
    };
    toggle.click();
    assert_eq!(toggle.describe(), "Dark mode: checked");

    let toggle = match *widget {
        WidgetNode::ToggleButton(toggle) => toggle,
        WidgetNode::Label(_) => panic!("expected ToggleButton"),
    };
    assert_eq!(&*toggle as *const ToggleButton as usize, original_address);

    println!("{}", toggle.paint());
    println!("clicks={}, same_address=true", toggle.clicks);
}
