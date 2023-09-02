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
    NewTest(args::Config),
    // Stuff goes here
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerError {
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientMessage {
    Request(ClientRequest),
    Response(ClientReply),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientRequest {
    NewTest(args::Config),
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
