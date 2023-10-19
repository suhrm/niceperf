import dummynet
import time
import logging
import pytest


log = logging.getLogger("test_tcp")
log.setLevel(logging.INFO)
log.info("starting test_icmp")


# TODO: This still needs some work.
@pytest.fixture(scope="session")
def ping_binary(pytestconfig):
    return pytestconfig.getoption("--ping_binary")


def test_tcp():
    process_monitor = dummynet.ProcessMonitor(log=log)
    shell = dummynet.HostShell(log=log, sudo=True, process_monitor=process_monitor)
    net = dummynet.DummyNet(shell=shell)

    try:
        namespaces = net.netns_list()
        assert "client" not in namespaces
        assert "server" not in namespaces

        client = net.netns_add("client")
        server = net.netns_add("server")

        net.link_veth_add(p1_name="eth-client", p2_name="eth-server")
        net.link_set("client", "eth-client")
        net.link_set("server", "eth-server")

        client.addr_add("10.0.0.1/24", "eth-client")
        server.addr_add("10.0.0.2/24", "eth-server")

        client.up(interface="eth-client")
        server.up(interface="eth-server")
        client.tc(interface="eth-client", loss=1)

        server.run_async(
            cmd="../target/debug/ping server tcp -i eth-server --dst-addr 10.0.0.2 --dst-port 1234",
            daemon=True,
        )

        time.sleep(0.5)

        res = client.run_async(
            cmd="../target/debug/ping client tcp  -i eth-client --dst-addr 10.0.0.2 --dst-port 1234 -c 1000 --interval 10",
        )

        while process_monitor.run():
            pass

        log.info(res.stdout)
        res.match(stdout="*seq=9*", stderr=None)

    finally:
        net.cleanup()
def test_tcp_timeout():
    process_monitor = dummynet.ProcessMonitor(log=log)
    shell = dummynet.HostShell(log=log, sudo=True, process_monitor=process_monitor)
    net = dummynet.DummyNet(shell=shell)

    try:
        namespaces = net.netns_list()
        assert "client" not in namespaces
        assert "server" not in namespaces

        client = net.netns_add("client")
        server = net.netns_add("server")

        net.link_veth_add(p1_name="eth-client", p2_name="eth-server")
        net.link_set("client", "eth-client")
        net.link_set("server", "eth-server")

        client.addr_add("10.0.0.1/24", "eth-client")
        server.addr_add("10.0.0.2/24", "eth-server")

        client.up(interface="eth-client")
        server.up(interface="eth-server")
        client.tc(interface="eth-client", delay=40)

        server.run_async(
            cmd="../target/debug/ping server tcp -i eth-server --dst-addr 10.0.0.2 --dst-port 1234",
            daemon=True,
        )

        time.sleep(0.5)

        res = client.run_async(
            cmd="../target/debug/ping client tcp  -i eth-client --dst-addr 10.0.0.2 --dst-port 1234 -c 1000 --interval 10",
        )

        while process_monitor.run():
            pass

        log.info(res.stdout)
        res.match(stdout="*seq=9*", stderr=None)

    finally:
        net.cleanup()
