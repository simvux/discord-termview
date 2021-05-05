FROM ubuntu:20.10

RUN mkdir /danger-zone/

COPY target/release/discord-termview /danger-zone/discord-termview

ARG TOKEN=my_token
ENV DISCORD_TOKEN=$TOKEN

ARG ROLES=1111;2222;3333
ENV ALLOWED_ROLES=$ROLES

ENTRYPOINT ["/danger-zone/discord-termview"]
