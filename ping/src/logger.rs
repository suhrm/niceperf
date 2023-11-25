use common::Logging;
#[derive(Debug, Logging, Clone, Default)]
pub struct PingResult {
    pub seq: u16,
    pub unique_seq: u128,
    pub ttl: u8,
    pub rtt: f64,
    pub size: usize,
    pub send_timestamp: u128,
    pub recv_timestamp: u128,
    pub dst_addr: String,
    pub src_addr: String,
}

#[derive(Debug, Logging, Clone, Default)]
pub struct UDPEchoResult {
    pub seq: u128,
    pub send_timestamp: u128,
    pub server_timestamp: u128,
    pub recv_timestamp: u128,
    pub rtt: f64,
    pub size: usize,
    pub src_addr: String,
    pub dst_addr: String,
}

#[derive(Debug, Logging, Default, Clone)]
pub struct TCPEchoResult {
    pub seq: u128,
    pub send_timestamp: u128,
    pub server_timestamp: u128,
    pub recv_timestamp: u128,
    pub rtt: f64,
    pub size: usize,
    pub src_addr: String,
    pub dst_addr: String,
    pub cc: String,
    pub retrans: u32,
}
