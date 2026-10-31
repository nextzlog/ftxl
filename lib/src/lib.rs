mod crc;
mod decoder;
mod engine;
mod ft;
mod ldpc;
mod message;
mod modem;
mod proto;
mod resample;
mod sequence;
mod tables;
#[cfg(test)]
mod vectors;

pub mod sim;

pub use engine::{
    next_slot, slot_start, Burst, Command, Engine, Notice, Pick, Proto, Qso, Setup, Spot, Warning,
};
pub use ft::{Ft4, Ft8};
pub use message::{fields, pack, unpack, Fields};
pub use modem::{Decode, FtOptions, Modem, Options, Parity, Transmission, MAX_HZ};
pub use sequence::{Contact, Sequencer, State, MESSAGES};

#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
struct ReadmeDoctests;
