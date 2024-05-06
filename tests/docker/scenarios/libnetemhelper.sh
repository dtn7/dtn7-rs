DOCKERCMD=docker
# use podman if available
if command -v podman &> /dev/null; then
  DOCKERCMD=podman
fi

set_on_all_interfaces() {
  for n in $NODES; do
    IFACES=$($DOCKERCMD exec $n cat /proc/net/dev | awk '{print $1}' | grep -E '^eth[0-9]+' | cut -d ':' -f1)
    CMDS=""
    for i in $IFACES; do
      CMDS="$CMDS  echo \"[$1] Setting loss on $n ($i) to $2\" && tc qdisc $1 dev $i root netem loss $2 && "	
    done
    $DOCKERCMD exec $n bash -c "${CMDS} true"
  done
}

set_on_all_interfaces add 0%


set_loss () {
  echo "Setting loss on $1 to $2"
  $DOCKERCMD exec $1 tc qdisc change dev eth0 root netem loss $2
}

loss_interval() {
  IFACE=eth0
  if [ -n "$4" ]; then
    IFACE=$4
  fi
  echo "Setting loss on $1 ($IFACE) to $2 for $3s"
  $DOCKERCMD exec $1 tc qdisc change dev $IFACE root netem loss $2
  sleep $3
  $DOCKERCMD exec $1 tc qdisc change dev $IFACE root netem loss 0%
}

cleanup () {
  echo "Cleaning up..."
  set_on_all_interfaces del 0%
}

trap cleanup EXIT
