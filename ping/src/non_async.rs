use common::*;
use polling::{Events, Poller};
use anyhow::Result;
fn run() -> Result<()> {
    let poller = Poller::new()?;
    let mut events = Events::new();

    loop {
        events.clear();
        poller.wait(&mut events, None)?;
        for event in events.iter() {
            println!("event: {:?}", event);
        }
    }
    Ok(())
}
