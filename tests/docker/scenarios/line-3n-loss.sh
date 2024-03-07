#!/bin/bash

while true; do
  echo "Setting loss on n3 to 100% for 30s" 
  docker run --rm  -v /var/run/docker.sock:/var/run/docker.sock gaiaadm/pumba netem --duration 30s loss -p 100 line-3n-n3-1 &
  sleep 30
  echo " n3 back to 0% loss." 
  echo "Setting loss on n1 to 100% for 30s"	
  docker run --rm  -v /var/run/docker.sock:/var/run/docker.sock gaiaadm/pumba netem --duration 30s loss -p 100 line-3n-n1-1 &
  sleep 30
  echo " n1 back to 0% loss."
done
