FROM golang:1.13 AS builder

COPY . /app
WORKDIR /app

ENV GO111MODULE=on

RUN GOOS=linux go build -o tarantool-operator cmd/manager/main.go

FROM centos:8 as runner

# install operator binary
COPY --from=builder /app/tarantool-operator /usr/local/bin/tarantool-operator
COPY --from=builder /app/build/bin/user_setup /usr/local/bin/user_setup

ENV OPERATOR=/usr/local/bin/tarantool-operator \
    USER_UID=1001 \
    USER_NAME=tarantool-operator

RUN  /usr/local/bin/user_setup

ENTRYPOINT ["/usr/local/bin/entrypoint"]

USER ${USER_UID}
