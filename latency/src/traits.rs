// use anyhow::Result;
// use enum_dispatch::enum_dispatch;
//
// use crate::tcp::{TcpClient, TcpLatency};
// #[enum_dispatch]
// pub enum LatencyProtocol {
//     Tcp(TcpLatency),
// }
//
// #[enum_dispatch(LatencyProtocol)]
// pub trait Latency {
//     async fn run(&mut self) -> Result<()>;
//     async fn accept(&mut self) -> Result<LatencyRW>;
// }
//
// #[enum_dispatch]
// pub enum LatencyRW {
//     Tcp(TcpClient),
// }
//
// #[enum_dispatch(LatencyRW)]
// pub trait LatencyHandler {
//     async fn read(&mut self) -> Result<()>;
//     async fn write(&mut self) -> Result<()>;
// }
