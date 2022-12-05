pub struct PingPacket {
    pub id: u64,
    pub snd_time: u64,
    pub rcv_time: u64,
    pub snd_offset: u64, // offset of the sender's clock. i.e. the if the sender
    // is rotating the periodicity. TODO: This could bea traffic model instead
    pub seq: u64,
    pub padding: Vec<u8>,
}

trait OWD {
    fn owd(&self) -> u64;
}
impl OWD for Packet {
    fn owd(&self) -> u64 {
        self.rcv_time - self.snd_time
    }
}
// TODO: Add jitter and other metrics
// Jitter is the difference between the current and previous OWD
// Potentially take a look at media over quic
