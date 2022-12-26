use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Packet {
    pub packet_type: PacketType,
    pub connection_id: u16
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