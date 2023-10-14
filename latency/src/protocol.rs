use std::time::UNIX_EPOCH;

use anyhow::Result;
use common::Logging;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LatencyMsg {
    id: u64,
    seq: u64,
    timestamp: u64,
    payload: Vec<u8>,
}
#[derive(Debug, Clone, Default, Logging)]
struct LatencyResult {
    id: u64,
    seq: u64,
    send_timestamp: u64,
    recv_timestamp: u64,
    payload_size: usize,
}

impl From<LatencyMsg> for LatencyResult {
    fn from(msg: LatencyMsg) -> Self {
        Self {
            id: msg.id,
            seq: msg.seq,
            send_timestamp: msg.timestamp,
            recv_timestamp: std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            payload_size: msg.payload.len() + 8 + 8 + 8,
        }
    }
}
