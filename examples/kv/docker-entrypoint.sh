#!/bin/bash
set -e

if [[ -z "${TARANTOOL_WORK_DIR}" ]]; then
    echo "TARANTOOL_WORK_DIR must be defined"
    exit 1
fi

if [[ -z "${TARANTOOL_RUN_DIR}" ]]; then
    echo "TARANTOOL_RUN_DIR was not provided. Setting default: ${TARANTOOL_WORK_DIR}/run"
    export TARANTOOL_RUN_DIR=$TARANTOOL_WORK_DIR/run
fi

mkdir -p $TARANTOOL_WORK_DIR $TARANTOOL_LOG_DIR $TARANTOOL_RUN_DIR

exec "$@"
