FROM tarantool/tarantool:1.x-centos7

# Tarantool binary protocol port
EXPOSE 3301
# Application HTTP API
EXPOSE 8081

ENV TARANTOOL_WORK_DIR=/var/lib/tarantool \
    TARANTOOL_RUN_DIR=/var/run \
    TARANTOOL_MEMTX_MEMORY=134217728

WORKDIR /opt/tarantool
RUN yum install -y gcc make cmake git unzip

COPY key-value-store ./
RUN tarantoolctl rocks make key-value-scm-1.rockspec

COPY ./docker-entrypoint.sh .

ENTRYPOINT ["/opt/tarantool/docker-entrypoint.sh"]

CMD ["tarantool", "/opt/tarantool/init.lua"]
