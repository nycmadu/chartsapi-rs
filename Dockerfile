FROM rust:latest as builder

RUN USER=root cargo new --bin rust-docker-web
WORKDIR ./rust-docker-web
COPY ./ .
RUN cargo build --release

FROM debian:bookworm-slim
ARG APP=/usr/src/app

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata openssl \
    && rm -rf /var/lib/apt/lists/*

EXPOSE 8000

ENV TZ=Etc/UTC \
    APP_USER=appuser

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY --from=builder /rust-docker-web/target/release/chartsapi-rs ${APP}/rust-docker-web

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

CMD ["./rust-docker-web"]
