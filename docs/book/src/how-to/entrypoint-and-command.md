# Entrypoint and command

A container image declares two things: an `ENTRYPOINT`, the executable it
runs, and a `CMD`, the arguments handed to that executable. The manifest
mirrors them as `entrypoint` and `command`.

The two are independent. Setting `entrypoint` alone leaves the image `CMD`
in place, so it is appended as arguments to the new entrypoint. Setting
`command` alone leaves the image `ENTRYPOINT` in place, so the value is
appended to it as arguments.

## Why a shim needs both

An image whose entrypoint is a binary cannot be given a startup script by
setting `command` alone:

```yaml
resources:
  app:
    dockerfile:
      context: .
      command: ["sh", "-c", "setup && exec /usr/local/bin/app"]
```

If the image declares `ENTRYPOINT ["/usr/local/bin/app"]`, the container
runs `/usr/local/bin/app sh -c "setup && exec /usr/local/bin/app"`. The
three strings reach the binary as positional arguments, which most argument
parsers reject. The script never runs.

Override the entrypoint as well:

```yaml
resources:
  app:
    dockerfile:
      context: .
      entrypoint: ["sh", "-c"]
      command: ["setup && exec /usr/local/bin/app"]
```

Now the container runs `sh -c "setup && exec /usr/local/bin/app"`.

This is the pattern for bridging an application that reads its
configuration from a file to LightShuttle's environment-variable service
discovery: write the file from the injected variable, then `exec` the
daemon.

## How each target spells it

The same two concepts carry different names depending on where a manifest is
exported. LightShuttle translates; you always write `entrypoint` and
`command`.

| Concept | Manifest | Docker Engine | Compose | Kubernetes |
| :--- | :--- | :--- | :--- | :--- |
| the executable | `entrypoint` | `Entrypoint` | `entrypoint` | `command` |
| its arguments | `command` | `Cmd` | `command` | `args` |

Kubernetes is the only target that crosses the names: its `command` is the
entrypoint, and its `args` is the `CMD`. This trips up readers who assume
the words mean the same thing everywhere, so an exported chart or manifest
will not look field-for-field like the source.

## Clearing an entrypoint

Not supported. `entrypoint: []` is rejected with an error. The Engine API
and Compose disagree on how to express a reset, and no use case has required
it so far.
