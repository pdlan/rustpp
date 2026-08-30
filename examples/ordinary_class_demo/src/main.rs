include!(concat!(env!("OUT_DIR"), "/generated.rs"));

fn main() {
    direct_parent();
    drop(Parent::construct_box());

    let owner = Parent::construct_rc();
    let clone = owner.clone();
    drop(owner);
    drop(clone);

    let owner = Parent::construct_arc();
    let clone = owner.clone();
    drop(owner);
    drop(clone);

    let derived: ABox = Derived::construct_box();
    assert!(__rpp_is_exact_derived(&*derived));
    let address = derived.__rpp_complete_address();
    let sibling = __rpp_cast_ref_a_to_b(&*derived).expect("cross-cast must succeed");
    assert_eq!(sibling.__rpp_complete_address(), address);
    let derived = __rpp_cast_box_a_to_derived(derived).unwrap_or_else(|_| panic!("downcast"));
    assert_eq!(derived.__rpp_complete_address(), address);

    let derived: ARc = Derived::construct_rc();
    let weak = std::rc::Rc::downgrade(&derived);
    let sibling = __rpp_cast_rc_a_to_b(derived).unwrap_or_else(|_| panic!("cross-cast"));
    assert_eq!(std::rc::Rc::strong_count(&sibling), 1);
    assert!(weak.upgrade().is_some());
}
