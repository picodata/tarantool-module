mkdir -p `pwd`/.run
docker network create -d bridge kv
docker run -d \
    --network=kv \
    -e TARANTOOL_HTTP_PORT=8082 \
    -e TARANTOOL_ADVERTISE_URI=router:3301 \
    -e TARANTOOL_WORK_DIR=/data/tarantool \
    -e TARANTOOL_INSTANCE_NAME=router_3 \
    -p 8082:8082 \
    --name=router \
    --cidfile=`pwd`/.run/router.cid \
    key-value-test
docker run -d \
    --network=kv \
    -e TARANTOOL_ADVERTISE_URI=storage:3301 \
    -e TARANTOOL_WORK_DIR=/data/tarantool \
    -e TARANTOOL_INSTANCE_NAME=storage_2 \
    --name=storage \
    --cidfile=`pwd`/.run/storage.cid \
    key-value-test
docker run -d \
    --network=kv \
    -e TARANTOOL_ADVERTISE_URI=topology:3301 \
    -e TARANTOOL_WORK_DIR=/data/tarantool \
    -e TARANTOOL_INSTANCE_NAME=topology_1 \
    -p 8081:8081 \
    --name=topology \
    --cidfile=`pwd`/.run/topology.cid \
    key-value-test
