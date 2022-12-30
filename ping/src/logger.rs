use std::{fmt, fs::File as StdFile};

use anyhow::Result;
use tokio::{fs::File, io::AsyncWriteExt};
pub struct PingLogger {
    logger: tokio::fs::File,
}

impl PingLogger {
    pub fn new(file_name: String) -> Result<PingLogger> {
        let logger = StdFile::create(file_name)?;
        let logger = File::from_std(logger);

        Ok(PingLogger { logger })
    }
    pub async fn log<T>(&mut self, msg: &T) -> Result<()>
    where
        T: fmt::Display,
    {
        self.logger.write_all(msg.to_string().as_bytes()).await?;
        Ok(())
    }
}
#[derive(Debug)]
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

impl fmt::Display for PingResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{},{},{},{}\n",
            self.seq,
            self.unique_seq,
            self.ttl,
            self.rtt,
            self.size,
            self.send_time,
            self.recv_time,
            self.dst_addr,
            self.src_addr
        )
    }
}

impl PingResult {
    pub fn header() -> String {
        "seq,unique_seq,ttl,rtt,size,send_time,recv_time,dst_addr,src_addr\n"
            .to_string()
    }
}

pub struct UDPEchoResult {
    pub seq: u128,
    pub send_timestamp: u128,
    pub server_timestamp: u128,
    pub recv_timestamp: u128,
    pub rtt: f64,
    pub src_addr: String,
    pub dst_addr: String,
}

impl fmt::Display for UDPEchoResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{},{}\n",
            self.seq,
            self.send_timestamp,
            self.server_timestamp,
            self.recv_timestamp,
            self.rtt,
            self.dst_addr,
            self.src_addr
        )
    }
}
