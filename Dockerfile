FROM scratch
LABEL APP ="Webb Relayer"
LABEL AUTHOR="Webb Developers <dev@webb.tools>"

ENV RUST_BACKTRACE=full
ENV WEBB_PORT=9955

VOLUME [ "/config" ]

ADD build/webb-relayer webb-relayer

EXPOSE ${WEBB_PORT}

CMD ["./webb-relayer", "-vvvv", "-c", "/config/config.toml"]
