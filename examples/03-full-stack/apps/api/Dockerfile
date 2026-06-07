# Minimal multi-stage Dockerfile standing in for a real API service.
# The manifest selects the `dev` stage through `target: dev`.

FROM alpine:3.20 AS base
WORKDIR /app

FROM base AS dev
EXPOSE 8080
CMD ["sh", "-c", "echo api (dev) listening on 8080 && sleep 3600"]

FROM base AS release
EXPOSE 8080
CMD ["sh", "-c", "echo api (release) listening on 8080 && sleep 3600"]
