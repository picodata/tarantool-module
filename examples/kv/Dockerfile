FROM centos:centos7 as key-value-build

WORKDIR /home/tarantool
COPY .bundles/tarantool-enterprise-bundle.tar.gz tarantool-enterprise-bundle.tar.gz
RUN tar -xzvf tarantool-enterprise-bundle.tar.gz && rm tarantool-enterprise-bundle.tar.gz

COPY key-value-store /home/tarantool/key-value-store
ENV PATH /home/tarantool/tarantool-enterprise:$PATH
RUN tarantoolctl rocks make key-value-store/key-value-scm-1.rockspec

#############

FROM registry.access.redhat.com/ubi7/ubi-minimal:latest as release

# Tarantool binary protocol port
EXPOSE 3301

# Application HTTP API
EXPOSE 8081

ENV TARANTOOL_WORK_DIR=/tarantool/data \
    TARANTOOL_RUN_DIR=/tarantool/run

WORKDIR /home/tarantool
COPY --from=key-value-build /home/tarantool .
COPY ./docker-entrypoint.sh .

ENTRYPOINT ["/home/tarantool/docker-entrypoint.sh"]

CMD ["tarantool", "./key-value-store/init.lua"]
