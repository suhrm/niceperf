import time
import dummynet
import logging


wan_entry = "wan-entry"
wan_exit = "wan-exit"


# create two network namespaces
# lan-node <-|dhcp|-> entry <--> exit <-|nat|-> wan-node
def configure_namespaces(net):
    entry_node = net.netns_add("entry")
    exit_node = net.netns_add("exit")
    lan_node = net.netns_add("lan-node")
    wan_node = net.netns_add("wan-node")
    # Create and connect wan veth pairs of entry and exit
    net.link_veth_add(wan_entry, wan_exit)
    net.link_veth_add("lan1", "lan2")
    net.link_veth_add("wan1", "wan2")
    net.link_veth_add("wan3", "wan4")
    net.link_veth_add("wan5", "wan6")
    net.link_set("entry", wan_entry)
    net.link_set("exit", wan_exit)
    net.link_set("entry", "lan2")
    net.link_set("lan-node", "lan1")
    # net.link_set("wan-node", "wan1")
    net.link_set("exit", "wan2")
    net.link_set("wan-node", "wan6")
    net.link_set("entry", "wan4")

    net.bridge_add("wan-bridge")
    net.bridge_set("wan-bridge", "wan1")
    net.bridge_set("wan-bridge", "wan3")
    net.bridge_set("wan-bridge", "wan5")
    net.bridge_up("wan-bridge")
    # REMBER to up both pairs of veth attached to a bridge or they won't work
    net.up("wan1")
    net.up("wan3")
    net.up("wan5")

    # Up the stuff
    lan_node.up("lan1")
    entry_node.up("lan2")
    exit_node.up("wan2")
    entry_node.up(wan_entry)
    exit_node.up(wan_exit)
    entry_node.up("wan4")
    wan_node.up("wan6")

    # Configure IP addresses
    lan_node.addr_add(interface="lan1", ip="10.0.0.2/20")
    entry_node.addr_add(interface="lan2", ip="10.0.0.1/20")
    exit_node.addr_add(interface="wan2", ip="10.1.0.1/20")
    entry_node.addr_add(interface="wan-entry", ip="10.3.0.1/20")
    exit_node.addr_add(interface="wan-exit", ip="10.3.0.2/20")
    entry_node.addr_add(interface="wan4", ip="10.1.0.3/20")
    wan_node.addr_add(interface="wan6", ip="10.1.0.4/20")

    # Configure routing


if __name__ == "__main__":
    log = logging.getLogger("dummynet")
    log.setLevel(logging.DEBUG)
    host = dummynet.HostShell(log=log, sudo=True, process_monitor=None)
    dummynet = dummynet.DummyNet(shell=host)
    try:
        configure_namespaces(dummynet)
        time.sleep(2)
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        dummynet.cleanup()
    finally:
        dummynet.cleanup()
