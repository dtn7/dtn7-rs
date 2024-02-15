#/bin/sh

docker build -t gh0st42/dtn7:bookworm -f Dockerfile.debian-bookwork-slim $@ ..

docker build -t gh0st42/dtn7:alpine -f Dockerfile.alpine-slim $@ ..