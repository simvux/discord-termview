#!/bin/env bash

cargo build --release
docker build --no-cache -t termview --build-arg TOKEN='my-discord-token' --build-arg ROLES='<id-of-role>' .

if [[ $* == *run* ]]
then
    docker run termview:latest
fi
