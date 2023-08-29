use common::Logging;
#[derive(Debug, Logging)]
pub struct PingResult {
    pub seq: u16,
    pub unique_seq: u128,
    pub ttl: u8,
    pub rtt: f64,
    pub size: usize,
    pub send_time: u128,
    pub recv_time: u128,
    pub dst_addr: String,
    pub src_addr: String,
}

#[derive(Debug, Logging)]
pub struct UDPEchoResult {
    pub seq: u128,
    pub send_timestamp: u128,
    pub server_timestamp: u128,
    pub recv_timestamp: u128,
    pub rtt: f64,
    pub src_addr: String,
    pub dst_addr: String,
}

#[derive(Debug, Logging)]
pub struct TCPEchoResult {
    pub seq: u128,
    pub send_timestamp: u128,
    pub server_timestamp: u128,
    pub recv_timestamp: u128,
    pub rtt: f64,
    pub src_addr: String,
    pub dst_addr: String,
    pub cc: String,
}
