# hello-static

A static Go program that prints one line. It is the first image the runner publishes from `docker/`.

```sh
docker pull sudtanj/hello-static:latest
docker run --rm sudtanj/hello-static:latest
```

The image supports `linux/amd64` and `linux/arm64`. Tags: `latest` and `sha-<12 hex>`, the first 12 hex digits of the commit that built it. The base is `gcr.io/distroless/static`; the process runs as user 65532 with no shell.

This folder is the only place to change the image. A merge to `main` rebuilds and republishes it.
