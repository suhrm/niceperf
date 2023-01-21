import dummynet
import logging
import pytest


log = logging.getLogger("test_icmp")
log.setLevel(logging.INFO)
log.info("starting test_icmp")


# TODO: This still needs some work.
@pytest.fixture(scope="session")
def ping_binary(pytestconfig):
    return pytestconfig.getoption("--ping_binary")


def test_icmp():
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

        res = client.run_async(
            cmd="../target/debug/ping client icmp -i eth-client --dst-addr 10.0.0.1 -c 2",
        )

        while process_monitor.run():
            pass

        assert res.returncode == 0

    finally:
        net.cleanup()
