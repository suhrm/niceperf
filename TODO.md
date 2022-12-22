# TODOs
## Latency
* icmp ping (similar to ping)
  - [ ] For ICMP we need both ip seqnr and an internal counter which does not wrap in the lifetime of the measurement
  - [ ] freqency of the measurement should be configurable
  - [ ] payload size should be configurable as well configuration that includes underlaying protocol headers (ipv4, ipv6)
  - [ ] output must include timestamp
  - [ ] ability to bind to a specific interface
  - [ ] amount of infligt pings should be configurable
  - [ ] IPv6 support
  - [ ] max count of pings should be configurable
  - [ ] timeout of pings should be configurable
* udp echo
  - [ ] Need own seqnr in packet
  - [ ] freqency of the measurement should be configurable
  - [ ] payload size should be configurable as well configuration that includes underlaying protocol headers (ipv4, ipv6)
  - [ ] output must include timestamp
  - [ ] ability to bind to a specific interface
  - [ ] amount of infligt pings should be configurable
  - [ ] IPv6 support
  - [ ] max count of pings should be configurable
  - [ ] timeout of pings should be configurable
  - [ ] ACK frequency should be configurable.
* tcp echo (for congestion control etc...)
  - [ ] Need own seqnr in packet
  - [ ] Configurable congestion control algorithm
  - [ ] MSS should be configurable
  - [ ] Payload size should be configurable
  - [ ] freqency of the measurement should be configurable
  - [ ]  TCP_NODELAY should always be sess
* OWD (one way delay)
  - [ ] TBD potenially take a look at Media over quic RFC.
* Output should be in a easily parsable format (json, bson (maybe))
* Configuration should happen via a cli or config file (json) POTENTIALLY multiple tests defined in one config file


## Throughput
* TCP throughput
    - [ ] Configurable congestion control algorithm
    - [ ] MSS should be configurable
    - [ ] Payload size should be configurable
    - [ ] bandwidth target should be configurable
    - [ ] pacing timer
    - [ ] multiple streams
    - [ ] Application level jitter
    - [ ] reporting interval should be configurables
    - [ ] bind to specific interface
    - [ ] port/ip address should be configurable

    - [ ] output: throughput, retransmissions, congestion window, application jitter.

* UDP throughput
    - [ ] Payload size should be configurable
    - [ ] pacing timer
    - [ ] bandwidth target should be configurable
    - [ ] reporting interval should be configurables
    - [ ] jitter
    - [ ] bind to specific interface
    - [ ] port/ip address should be configurable

    - [ ] output: throughput, jitter, loss, out of order.

* QUIC throughput
  - [ ] Configurable congestion control algorithm
  - [ ] MSS should be configurable
  - [ ] Payload size should be configurable
  - [ ] bandwidth target should be configurable
  - [ ] pacing timer
  - [ ] multiple streams
  - [ ] Application level jitter
  - [ ] reporting interval should be configurables
  - [ ] bind to specific interface
  - [ ] port/ip address should be configurable
  - [ ] both support of stream and datagram.

  - [ ] output: throughput, retransmissions, congestion window, application jitter.

* the control channel should be configurable to i.e another port or even ip address
* affinity to specific cores should be configured via e.g. taskset



# Additional Notes
- [ ] iperf fq-rate is not the same as the actual throughput?
- [ ] enable/disable of nagle algorithm
- [ ] set don't fragment bit on ipv4 headers
- [ ] get server output iperf?
- [ ] server addrress should be public ip/reachable from outside
  - [ ] In case of udp reply to the same address as the request came from 