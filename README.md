# Cronify

A lightweight single-job cron process.

Example:

```
# Runs `echo Hello, world!` once a minute
cronify "* * * * *" echo "Hello, world!"

# Runs `echo Hello, world!` once every 15 minutes
cronify "*/15 * * * *" echo "Hello, world!"
```

Tip: <https://crontab.guru>

## Use in Docker

Rather than installing a whole cron daemon inside your container, or
using cron on your host to trigger your containers, Cronify can be used
as an alternate `CMD` or `ENTRYPOINT` to your main command.

For example:

```
FROM rust:1.31 AS cronify
WORKDIR /usr/src/cronify
ADD https://github.com/chris13524/cronify/archive/master.tar.gz /usr/src/cronify
RUN cargo build --release

FROM ubuntu:latest
COPY --from=cronify /usr/src/cronify/target/release/cronify /usr/local/bin
ENTRYPOINT ["/usr/local/bin/cronify", "* * * * *", "echo", "Hello, world!"]
```
