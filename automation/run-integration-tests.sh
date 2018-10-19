#!/bin/sh -ex

EXEC_PATH=$(dirname "$(realpath "$0")")
PROJECT_PATH="$(dirname $EXEC_PATH)"
DOCKER_IMAGE="nmstate/centos7-nmstate-dev"

NET0="nmstate-net0"
NET1="nmstate-net1"

test -t 1 && USE_TTY="-t"

function remove_container {
    res=$?
    [ "$res" -ne 0 ] && echo "*** ERROR: $res"
    docker stop $CONTAINER_ID
    docker rm $CONTAINER_ID
    docker network rm $NET0
    docker network rm $NET1
}

function pyclean {
        find . -type f -name "*.py[co]" -delete
        find . -type d -name "__pycache__" -delete
}

cd "$EXEC_PATH"

CONTAINER_ID="$(docker run --privileged -d -v /sys/fs/cgroup:/sys/fs/cgroup:ro -v $PROJECT_PATH:/workspace/nmstate $DOCKER_IMAGE)"
trap remove_container EXIT
docker exec $USE_TTY -i $CONTAINER_ID /bin/bash -c 'systemctl start dbus.socket'

docker network create $NET0 || true
docker network create $NET1 || true
docker network connect $NET0 $CONTAINER_ID
docker network connect $NET1 $CONTAINER_ID
docker exec $USE_TTY -i $CONTAINER_ID /bin/bash -c 'ip addr flush eth1 && ip addr flush eth2'
docker exec $USE_TTY -i $CONTAINER_ID /bin/bash -c 'nmcli dev; nmcli con; ip addr; ip route; cat /etc/resolv.conf; ping -c 3 github.com || true'

pyclean
docker exec $USE_TTY -i $CONTAINER_ID /bin/bash -c '
  cd /workspace/nmstate &&
  pip install . &&
  pytest \
    --log-level=DEBUG \
    --durations=5 \
    --cov=libnmstate \
    --cov=nmstatectl \
    --cov-report=html:htmlcov-py27 \
    tests/integration
'
