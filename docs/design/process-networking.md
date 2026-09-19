# Process resources: networking and discovery

This note locks the discovery model between containers and native processes before any
implementation of the `process` resource kind. It exists because the naive answer, "give the
process a hostname like every other resource", cannot work: a native process never joins the
project bridge network, so the embedded DNS server that resolves `db` or `api` inside containers
knows nothing about it.

## What the runtime provides today

Every container started by LightShuttle is attached to a per-project bridge network and carries a
network alias equal to its resource name. Container to container discovery therefore needs nothing
beyond the resource name: `api` reaches `db:5432` because Docker resolves `db` on that network.

Published ports are bound to `127.0.0.1` by default, so a stack is reachable from the host but not
from the local network.

A native process sits outside both mechanisms. It has no network alias, and it is not on the
bridge at all.

## Measurements

Measured on 2026-09-18 with a Python HTTP server as the native process, against two real daemons:
Docker Desktop 29.8.0 on Windows, and a Docker 29.8.1 Linux engine. The container side is a
`curl` image requesting `http://host.docker.internal:<port>/`.

| Process bound to | Docker Desktop (Windows) | Linux engine |
| --- | --- | --- |
| `127.0.0.1` | reachable (200) | unreachable (timeout) |
| `0.0.0.0` | reachable (200) | reachable (200) |
| Project network gateway | bind refused by the host | reachable (200) |

Four further facts came out of the same run:

1. On Docker 29, `host.docker.internal` resolves inside containers **without**
   `--add-host host.docker.internal:host-gateway`, including on a user-defined network. It resolves
   to an IPv6 address on the bridge. With the explicit alias, it resolves to the IPv4 gateway
   address instead. Both work. Older engines do not provide the name at all, so the alias is still
   added for them.
2. On the Linux engine, the project network gateway (`172.19.0.1`, carried by the `br-*` interface)
   is reachable from containers on that network **and** from the host itself. It is not reachable
   from the local network.
3. On Docker Desktop, a host process cannot bind to that gateway address at all: the bridge lives
   inside the Desktop virtual machine, and the address does not exist on the host. Binding returns
   `the requested address is not valid in its context`.
4. On the Linux engine, a process bound to the gateway is not reachable through `127.0.0.1`. The
   developer reaches it at the gateway address, which is a local interface of their machine.

The decisive fact is the first row of the table: the same manifest, with a process bound to
loopback, works on Docker Desktop and fails on a Linux engine. The difference is not DNS, it is the
listening interface. Desktop routes the request through a proxy inside its virtual machine, which
exits on the host loopback; a Linux engine sends the packet from the bridge to the gateway address,
where a loopback-bound socket never sees it.

## The model

Discovery is resolved per direction, and neither direction gives a process a hostname on the bridge.

### Process to container

The process reaches containers through their published ports on `127.0.0.1`. Nothing new is
required: the ports are already published there.

The consequence is a constraint on the manifest rather than on the runtime. A container a process
depends on must publish the port the process uses. A dependency on a container that publishes no
port is refused, naming both resources, because there is no address to hand out.

### Container to process

Containers reach the process through `host.docker.internal`, and LightShuttle sets
`host.docker.internal:host-gateway` on every container it starts. On Docker 29 the name already
resolves, so the alias is redundant there; it is set anyway, unconditionally, because it is what
makes older engines behave the same way, and because a conditional that depends on the daemon
version is one more thing to get wrong.

### The bind address LightShuttle provides

The runtime creates the project network, so it knows its gateway. It exports
`LIGHTSHUTTLE_BIND_ADDRESS` into the process environment, and the documented contract is that a
server started as a `process` resource binds to that address.

| Platform | Value | Why |
| --- | --- | --- |
| Docker Desktop (Windows, macOS) | `127.0.0.1` | The gateway is not bindable from the host, and Desktop reaches loopback anyway. |
| Linux engine | Project network gateway | Reachable from containers and from the developer's own machine, never from the local network. |

This keeps the not-exposed-by-default posture that published ports already have. Binding to
`0.0.0.0` would work everywhere in one line, and is rejected for exactly one reason: it publishes a
development service to every machine on the local network, which is what binding published ports to
loopback exists to prevent.

The contract has a limit worth stating plainly: LightShuttle cannot force a program to honour the
variable. A server hardcoding `127.0.0.1` stays unreachable from containers on a Linux engine. The
runtime therefore probes the process once it is up, and reports a failure that names the address it
provided, the address the process actually listened on, and the fix.

## Reference rendering

`${resources.<name>.host}` is resolved for the consumer, not for the resource:

| Consumer | Target | Rendered as |
| --- | --- | --- |
| Container | Container | The resource name, resolved by the bridge DNS |
| Container | Process | `host.docker.internal` |
| Process | Container | `127.0.0.1` with the published host port |
| Process | Process | `127.0.0.1` with the process port |

A reference therefore no longer renders identically everywhere, which is a real cost of this model:
reading a manifest no longer tells you the address a given service sees without knowing which side
asks. The alternative that preserves one name for both sides is an ambassador container per
process, relaying the resource name to the host. It was weighed and set aside: it buys transparency
with a container, a lifecycle and a health probe per process, plus one more network hop to explain
whenever something breaks.

## Failure modes

| Symptom | Cause | What LightShuttle reports |
| --- | --- | --- |
| Container cannot reach the process, Linux engine only | Process bound to loopback rather than the provided address | The provided address, the observed one, and the fix |
| Process cannot reach a container | The container publishes no port | Refusal naming the dependent process and the container |
| Host port already taken | Another project or a stray process holds it | The port, the resource claiming it, and the holder if it can be identified |
| Works on one machine, fails on another | Platform difference documented above | The daemon platform, in the same diagnostic |

## Export targets

An export of a manifest holding a `process` resource is refused, naming that resource. A native
process has no equivalent in Compose, Kubernetes or Helm, and emitting the rest as if the project
were complete would produce an artifact that misrepresents it. This follows the rule already
applied to a Compose service depending on a resource excluded from the export.

## Out of scope

- The `process` resource kind itself: its manifest shape, lifecycle, logs and health probing.
- Reaching a process from a container on a network LightShuttle did not create.
- Rootless daemons and remote daemons, where the host a container reaches is not the machine
  running the process. Both are refused rather than guessed at, until someone measures them.
