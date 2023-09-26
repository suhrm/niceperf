use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::args;

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerResponse {
    Ok(ServerReply),
    Error(ServerError),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerReply {
    NewTest(u64, args::Config),
    Handshake(u64),
    // Stuff goes here
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerError {
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientMessage {
    NewTest(u64, args::Config),
    Response(ClientReply),
    Handshake(ClientHandshake),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientHandshake {
    pub id: u64,
    pub protocol: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientRequest {
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientReply {
    Ok(ClientReplyOk),
    Error(ClientReplyError),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientReplyOk {
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientReplyError {
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum TestType {
    Tcp,
    Udp,
    Quic,
}
