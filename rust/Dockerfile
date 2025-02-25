FROM rust:1.70-slim as BUILDER
ARG PACKAGE
WORKDIR /build/
COPY . ./
RUN --mount=type=cache,target=./target \
    --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/rustup,sharing=locked \
    apt update && apt install -y musl-tools && \
    cargo build -p $PACKAGE --release --target x86_64-unknown-linux-musl

RUN --mount=type=cache,target=./target \
    mv ./target/x86_64-unknown-linux-musl/release/$PACKAGE /usr/local/bin/$PACKAGE

FROM alpine:3.18
ARG PACKAGE
WORKDIR /app/
COPY --from=BUILDER /usr/local/bin/$PACKAGE .
ENV RUST_BACKTRACE=1
ENV PATH "/app:$PATH"
ENV PACKAGE_NAME ${PACKAGE}
# Some black magics here:
# we need to use `/bin/sh -c` so that the env variable is correctly replaced
# but then everything in `CMD` is placed after the executed string, so we need
# to move it inside, these are passed as the variables `$0`, `$1`, `$2`, etc...
# this means that this will ignore after the first arguments
# if we ever combine this with `CMD` in exec form  so always use shell form
# (Note we could use shell-form here, but this is the same made explicit)
ENTRYPOINT ["/bin/sh", "-c", "$PACKAGE_NAME $0"]
# *sigh* if we don't add this $0 becomes /bin/sh in the command above
CMD [""]
