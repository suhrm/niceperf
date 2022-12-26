use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use anyhow::Result;



#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Packet {
    packet_type: PacketType,
    connection_id: u16
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum PacketType {
    SideChannel(SideChannel),
    Throughput(Throughput)
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct SideChannel {

}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum SideChannelMessageTypes {
    MeasurementReport(MeasurementReport),
    MeasurementStatus(MeasurementStatus)
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct MeasurementReport {

}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct MeasurementStatus {

}


#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Throughput {
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>
}