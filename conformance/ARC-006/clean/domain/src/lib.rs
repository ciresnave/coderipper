//! Three public items, inside a budget of four; the rest is private.
pub fn one() {}
pub fn two() {}
pub struct Three;

fn helper() {}
pub(crate) fn also_hidden() {}
