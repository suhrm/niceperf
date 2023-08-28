use std::{fmt, fs::File as StdFile};

use anyhow::Result;
use header::Logging;
use tokio::{fs::File, io::AsyncWriteExt};
pub trait Logging {
    fn header(&self) -> String;
}
pub struct PingLogger {
    logger: tokio::fs::File,
    header_written: bool,
}

impl PingLogger {
    pub fn new(file_name: String) -> Result<PingLogger> {
        let logger = StdFile::create(file_name)?;
        let logger = File::from_std(logger);

        Ok(PingLogger {
            logger,
            header_written: false,
        })
    }
    pub async fn log<T>(&mut self, msg: &T) -> Result<()>
    where
        T: fmt::Display + Logging,
    {
        if !self.header_written {
            self.logger.write_all(msg.header().as_bytes()).await?;
            self.header_written = true;
        }
        self.logger.write_all(msg.to_string().as_bytes()).await?;
        Ok(())
    }
}
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
