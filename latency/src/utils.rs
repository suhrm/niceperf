use common;
use polling;
use quinn_proto;
use rustix::time;

mod test {
    use std::{collections::HashMap, net::SocketAddr, sync::Arc};

    use bytes::{Bytes, BytesMut};
    use rustix::fd::{AsFd, AsRawFd};
    use timerfd::TimerState;

    use super::*;
    #[test]
    fn quinn_test() {
        let mut incomming_connections = HashMap::new();

        let now = || std::time::Instant::now();
        let client_addr: SocketAddr = "127.0.0.1:12346".parse().unwrap();
        let server_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let client_config = common::configure_client(None).unwrap();
        let server_config = common::configure_server().unwrap();

        let client_ep_cfg = quinn_proto::EndpointConfig::default();
        let server_ep_cfg = quinn_proto::EndpointConfig::default();

        let mut client_ep =
            quinn_proto::Endpoint::new(Arc::new(client_ep_cfg), None, false);
        let mut server_ep = quinn_proto::Endpoint::new(
            Arc::new(server_ep_cfg),
            Some(Arc::new(server_config.0)),
            true,
        );

        const CLIENT_ID: usize = 0;
        const SERVER_ID: usize = 1;

        // Create event loop {

        let poller = polling::Poller::new().unwrap();

        let server_socket = std::net::UdpSocket::bind(server_addr).unwrap();
        server_socket.set_nonblocking(true).unwrap();
        let client_socket = std::net::UdpSocket::bind(client_addr).unwrap();
        client_socket.set_nonblocking(true).unwrap();

        let mut timer = timerfd::TimerFd::new_custom(
            timerfd::ClockId::Monotonic,
            true,
            false,
        )
        .unwrap();

        timer.set_state(
            TimerState::Oneshot(std::time::Duration::new(1, 0)),
            timerfd::SetTimeFlags::Default,
        );

        let mut events = polling::Events::new();

        unsafe {
            poller
                .add(&client_socket, polling::Event::readable(CLIENT_ID))
                .unwrap();
            poller
                .add(&server_socket, polling::Event::readable(SERVER_ID))
                .unwrap();
            poller.add(&timer, polling::Event::readable(2)).unwrap();
        }

        let mut server = |recv_from: SocketAddr, recv_data: BytesMut| {
            let dgram_event = server_ep
                .handle(
                    now(),
                    recv_from,
                    Some(server_addr.ip()),
                    None,
                    recv_data,
                )
                .unwrap();
            match dgram_event {
                quinn_proto::DatagramEvent::ConnectionEvent(ch, event) => {
                    println!("Server connection event");
                    dbg!(&event);
                    let qconn: &mut quinn_proto::Connection =
                        incomming_connections.get_mut(&ch).unwrap();

                    qconn.handle_event(event);

                    let epevent = qconn.poll_endpoint_events();
                    dbg!(&epevent);
                    let appevent = qconn.poll();
                    dbg!(&appevent);

                    let transmit = qconn.poll_transmit(now(), 100).unwrap();
                    dbg!(&transmit);

                    server_socket
                        .send_to(&transmit.contents, transmit.destination)
                        .unwrap();
                }
                quinn_proto::DatagramEvent::NewConnection(
                    con_handle,
                    mut qconn,
                ) => {
                    println!("New connection");

                    // Insert connection into hashmap
                    // Poll transmit

                    while let Some(transmits) = qconn.poll_transmit(now(), 100)
                    {
                        println!("transmit");
                        dbg!(&transmits);
                        server_socket
                            .send_to(&transmits.contents, transmits.destination)
                            .unwrap();
                    }
                    incomming_connections.insert(con_handle, qconn);
                }
                quinn_proto::DatagramEvent::Response(transmit) => {
                    println!("Response");
                    dbg!(&transmit);
                    server_socket
                        .send_to(&transmit.contents, transmit.destination)
                        .unwrap();
                }
            }
        };
        let (conn_handle, mut client_conn) = client_ep
            .connect(
                now(),
                client_config,
                "127.0.0.1:12345".parse().unwrap(),
                "localhost",
            )
            .unwrap();
        let transmits = client_conn.poll_transmit(now(), 100).unwrap();
        // Send the first packet
        let recv_data = BytesMut::from(transmits.contents.clone().as_ref());
        client_socket
            .send_to(&recv_data, transmits.destination)
            .unwrap();

        let mut client = |recv_from: SocketAddr, recv_data: BytesMut| {
            let dgram_event = client_ep
                .handle(
                    now(),
                    recv_from,
                    Some(server_addr.ip()),
                    None,
                    recv_data,
                )
                .unwrap();
            match dgram_event {
                quinn_proto::DatagramEvent::ConnectionEvent(ch, cevent) => {
                    print!("connection event");
                    dbg!(&cevent);
                    (&mut client_conn).handle_event(cevent);

                    let epevent = (&mut client_conn).poll_endpoint_events();
                    if let Some(epevent) = epevent {
                        dbg!(&epevent);
                        client_ep.handle_event(ch, epevent);
                    }
                    let appevent = client_conn.poll();
                    client_conn.poll_timeout();

                    dbg!(&appevent);

                    let transmit =
                        (&mut client_conn).poll_transmit(now(), 100).unwrap();
                    dbg!(&transmit);

                    client_socket
                        .send_to(&transmit.contents, transmit.destination)
                        .unwrap();
                }
                quinn_proto::DatagramEvent::NewConnection(
                    con_handle,
                    mut qconn,
                ) => {
                    println!("New connection");
                }
                quinn_proto::DatagramEvent::Response(_) => todo!(),
            }
        };

        // Kick of the connection

        // Poll transmit

        loop {
            events.clear();
            poller.wait(&mut events, None).unwrap();
            for ev in events.iter() {
                match ev.key {
                    CLIENT_ID => {
                        let mut buf = [0u8; 2048];
                        let (len, recv_from) =
                            client_socket.recv_from(&mut buf).unwrap();

                        let recv_data = BytesMut::from(&buf[..len]);
                        client(recv_from, recv_data);

                        // Do client stuff
                        poller
                            .modify(
                                &client_socket,
                                polling::Event::readable(CLIENT_ID),
                            )
                            .unwrap();
                    }
                    SERVER_ID => {
                        // Do server stuff
                        let mut buf = [0u8; 2048];
                        let (len, recv_from) =
                            server_socket.recv_from(&mut buf).unwrap();
                        let recv_data = BytesMut::from(&buf[..len]);
                        server(recv_from, recv_data);

                        poller
                            .modify(
                                &server_socket,
                                polling::Event::readable(SERVER_ID),
                            )
                            .unwrap();
                    }
                    2 => {
                        println!("Timer fired");
                    }
                    _ => unreachable!(),
                }
            }
        }
        poller.delete(&client_socket).unwrap();
        poller.delete(&server_socket).unwrap();
    }
}
