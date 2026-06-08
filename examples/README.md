# LightShuttle examples

Each subfolder contains a self-contained `lightshuttle.yml` plus a
short `README.md` explaining what the manifest demonstrates and how to
run it.

| # | Example | Demonstrates |
|---|---|---|
| 01 | [`01-hello-world`](01-hello-world/) | The smallest possible manifest. A single Postgres. |
| 02 | [`02-postgres-and-api`](02-postgres-and-api/) | Interpolation, implicit dependencies, `LSH_<DEP>_<PROP>` auto-env. |
| 03 | [`03-full-stack`](03-full-stack/) | All four resource kinds in one realistic stack. |
| 04 | [`04-export`](04-export/) | The `export:` section and `lightshuttle export compose\|kubernetes\|helm`. |
| 05 | [`05-secrets`](05-secrets/) | `${env.*}` references, the `.env` file, `secrets check`, fail-fast boot. |
| 06 | [`06-dashboard-observability`](06-dashboard-observability/) | `dashboard:` on a fixed port, the `observability:` knob, a custom `healthcheck:`. |

Run any of them from inside the example directory:

```sh
cd examples/01-hello-world
lightshuttle up
```

For a guided walkthrough see the
[getting started tutorial](https://nubster-opensources.github.io/lightshuttle/tutorials/getting-started.html).
