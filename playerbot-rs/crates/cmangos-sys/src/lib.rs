//! Raw FFI bindings to `cpp_wrapper/botffi.h`.
//!
//! This crate is the single point of contact with the C contract. It contains
//! no logic — only the bindgen output: POD structs (`BotPosition`,
//! `BotWorldSnapshot`, `BotUnitSnapshot`, …) and the `BotCallbacks` vtable.
//!
//! Safe wrappers, the `World` trait, and the `MockWorld` test impl all live in
//! the sibling `cmangos` crate.

#![no_std]
#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code)]
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
