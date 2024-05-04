#!/bin/sh

# Sudo is needed for this script to work so make the check
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

# Delete network namespaces if they exist

ip netns del lan-node
ip netns del wan-node
ip netns del entry
ip netns del exit
ip link del dev wan-bridge
ip link del dev lan1
ip link del dev wan1
ip link del dev wan3
ip link del dev wan5
ip link del dev wan-entry

# Create 4 vertical panes in tmux 

tmux new-session -d -s test_session
tmux split-window -v -t test_session
# tmux split-window -h -t test_session
# tmux split-window -h -t test_session
# Run python script to create the 4 network namespace

python test_namespace.py &

# Open the network namespace in each pane
sleep 4
tmux send-keys -t test_session:1.1 'sudo -E ip netns exec lan-node $SHELL' Enter
tmux send-keys -t test_session:1.2 'sudo -E ip netns exec entry $SHELL' Enter
# tmux send-keys -t test_session:1.3 'ip netns exec exit fish' Enter
# tmux send-keys -t test_session:1.4 'ip netns exec wan-node fish' Enter


tmux attach -t test_session

