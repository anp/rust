#![feature(track_caller)] //~ WARN the feature `track_caller` is incomplete

#[track_caller]
extern "C" fn f() {}
//~^^ ERROR `#[track_caller]` requires Rust ABI

fn main() {}
