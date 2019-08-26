FROM centos:centos7

WORKDIR /home/tarantool
COPY yum-tnt.sh .
RUN ./yum-tnt.sh
RUN yum install -y epel-release && \
	curl --silent --location https://rpm.nodesource.com/setup_8.x | bash - && \
	yum install -y git make nodejs cmake gcc tarantool tarantool-devel

COPY key-value-store /home/tarantool/key-value-store
RUN tarantoolctl rocks install https://raw.githubusercontent.com/rosik/cartridge/master/cartridge-scm-1.rockspec

# Tarantool binary protocol port
EXPOSE 3301

# Application HTTP API
EXPOSE 8081

ENV TARANTOOL_WORK_DIR=/tarantool/data \
    TARANTOOL_RUN_DIR=/tarantool/run \
    TARANTOOL_MEMTX_MEMORY=134217728

COPY ./docker-entrypoint.sh .

ENTRYPOINT ["/home/tarantool/docker-entrypoint.sh"]

CMD ["tarantool", "./key-value-store/init.lua"]
