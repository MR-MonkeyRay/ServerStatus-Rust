FROM alpine:3.19.1

LABEL maintainer="mr.monkeyray@gmail.com" \
    description="A simple server monitoring tool's client"

ARG VERSION=v1.8.2

RUN apk add --no-cache unzip curl vnstat iproute2 procps \
    && curl -fSL -o /tmp/client.zip \
       "https://github.com/zdz/ServerStatus-Rust/releases/download/${VERSION}/client-x86_64-unknown-linux-musl.zip" \
    && unzip /tmp/client.zip -d /tmp/client \
    && mv /tmp/client/stat_client /usr/local/bin/stat_client \
    && chmod +x /usr/local/bin/stat_client \
    && rm -rf /tmp/client.zip /tmp/client

ENV RUST_BACKTRACE=1 \
    SERVER_HOST=127.0.0.1 \
    SERVER_PORT=8080 \
    GID=group1 \
    PASSWORD=changeme \
    ALIAS=unknown \
    INTERVAL=5 \
    HOST_TYPE=kvm \
    IP_SOURCE=ip-api.com

ENTRYPOINT ["/bin/sh", "-c", "exec stat_client \
    -a \"${SERVER_HOST}:${SERVER_PORT}/report\" \
    -g \"${GID}\" \
    -p \"${PASSWORD}\" \
    --alias=\"${ALIAS}\" \
    --interval=\"${INTERVAL}\" \
    -n \
    -t \"${HOST_TYPE}\" \
    --ip-source \"${IP_SOURCE}\""]
