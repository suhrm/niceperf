use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::icmp_new::Payload;

use anyhow::{anyhow, Result};
use common::{interface_to_ipaddr, ICMPSocket, Logger, Statistics};
use etherparse::{IcmpEchoHeader, Icmpv4Header, Icmpv4Type};

use crate::{args, icmp_new::{ICMPClientHandle, ICMPEcho}, logger::PingResult};
pub struct ICMPClient {
    /// Logger
    logger: Option<Logger<PingResult>>,
    /// Common options
    common: args::CommonOpts,
    /// ICMP handle
    handle: ICMPClientHandle,
    /// Rtt statistics
    rtt_stats: Statistics,
    /// Internal counter for sequence number of ICMP packets (Not the same as
    /// ip sequence number)
    internal_couter: u128,
}

impl ICMPClient {
    pub fn new(args: args::ICMPOpts) -> Result<ICMPClient> {
        let dst_addr = if let IpAddr::V4(addr) = args.common_opts.dst_addr {
            addr
        } else {
            return Err(anyhow!("IPv6 is not supported yet"));
        };
        // let src_addr = match args.common_opts.src_addr. {
        //     IpAddr::V4(addr) => addr,
        //     IpAddr::V6(_addr) => {
        //         return Err(anyhow!("IPv6 is not supported yet"));
        //     }
        // };

        // Safety: Safe to unwrap because we have a default value
        println!(
            "Pinging {} with {} bytes of data",
            dst_addr,
            args.common_opts.len.unwrap()
        );
        println!(
            "interval {} ms, preload {} packets ",
            args.common_opts.interval.unwrap(),
            args.common_opts.preload.unwrap()
        );
        // Safety: Safe to unwrap because we have a default value
        let payload = vec![0u8; args.common_opts.len.unwrap() - 8];
        if payload.len() < 16 {
            Err(anyhow!("Payload is too small"))?;
        }
        // // TODO: Generate a random payload for the ICMP packet

        let logger = match args.common_opts.file.clone() {
            Some(file_name) => Some(Logger::new(file_name)?),
            None => None,
        };
        let handle = ICMPClientHandle::new(&args);

        Ok(ICMPClient {
            logger,
            common: args.common_opts,
            handle,
            rtt_stats: Statistics::new(),
			internal_couter: 0,
        })
    }
	pub async  fn handle_result(&mut self, result: PingResult) -> Result<()> {
		self.rtt_stats.update(result.rtt);
		if let Some(logger) = &mut self.logger {
			logger.log(&result).await?;
		} else {
			println!(
				"{} bytes from {}: icmp_pay_seq={} time={:.3} ms ",
				result.size,
				result.src_addr,
				result.seq,
				result.rtt,
			);
		}
		Ok(())
			
	}
}

pub struct Runner{}
impl Runner{

 pub async fn run(mut client: ICMPClient) -> Result<()> {
    // TODO: Add support for timeout
    let _timeout_tracker =
        tokio::time::interval(std::time::Duration::from_millis(10 * 1000));
    let mut pacing_timer = tokio::time::interval(
        std::time::Duration::from_millis(client.common.interval.unwrap()),
    );

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = pacing_timer.tick() => {
					let send_timestamp = 
     std::time::Duration::from(nix::time::clock_gettime(
        nix::time::ClockId::CLOCK_MONOTONIC,
    )?).as_nanos() as u128;
					let echo_packet = ICMPEcho{
						icmp: IcmpEchoHeader{
							id: client.handle.identifier(),
							seq: client.internal_couter as u16,
						},
						payload: Payload{
							data: vec![0u8; client.common.len.unwrap() - 8],
							seq: client.internal_couter,
							timestamp: send_timestamp,

						},
					};
					client.handle.send(echo_packet).await?;
					client.internal_couter += 1;
					

},
                            Some(result) = client.handle.recv() => {
								client.handle_result(result).await?;
							

                            }

            }
        }
        Ok::<(), anyhow::Error>(())
    })
    .await?;
    Ok(())
}
}
