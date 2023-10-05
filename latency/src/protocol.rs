use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::args;

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageType {
    NewTest(u64, args::Config),
    Handshake(u64),
    Error(Error),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ErrorType {
    HandshakeFailed,
    TestFailed,
    Unknown,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Error(ErrorType);
