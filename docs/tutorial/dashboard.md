# Dashboard walkthrough

The LightShuttle dashboard is served on the same `127.0.0.1:<port>`
the orchestrator boots when you run `lightshuttle up`. The URL is
printed to the terminal and saved to `.lightshuttle/control.url` for
discovery by other client commands. This walkthrough boots a small
three-resource stack and visits each dashboard page.

## 1. Boot a three-resource stack

Use the example shipped at `examples/03-full-stack`:

```yaml
project:
  name: app
resources:
  db:
    postgres:
      version: '16'
  cache:
    redis:
      version: '7'
  api:
    container:
      image: alpine
      depends_on: [db, cache]
```

From that directory:

```bash
lightshuttle up
```

The boot log advertises the dashboard URL in colour, for example:

```text
LightShuttle dashboard ready at http://127.0.0.1:54321/
```

Open that URL in a browser.

## 2. Index page (`/`)

The index lists every resource declared in the manifest:

| Name  | Kind     | Status   | Healthy | Image          | Actions |
| ----- | -------- | -------- | ------- | -------------- | ------- |
| db    | postgres | running  | yes     | postgres:16-alpine | Restart |
| cache | redis    | running  | yes     | redis:7-alpine     | Restart |
| api   | container| starting | no      | alpine:3.20        | Restart |

- The status column refreshes every two seconds via an HTMX poll on
  `/_partials/resources`. No full page reload, no flicker.
- Clicking **Restart** posts to
  `/api/resources/{name}/restart` and returns 202 immediately. The
  poll picks up the cycle on the next refresh.
- Each row name is a link to the detail page.

## 3. Resource detail (`/resources/api`)

The detail page shows the full metadata block:

- Kind: `container`
- Status: `running`
- Healthy: yes
- Image: `alpine:3.20`
- Last error: only when a terminal failure has occurred.

Below the metadata, a live log pane streams `stdout` and `stderr` of
the resource via the `/ws/logs/{name}` WebSocket. The pane scrolls
automatically; `[stderr]` entries are prefixed for distinction.

The page also exposes a **Restart this resource** button targeting
the same REST endpoint as the index.

## 4. Visual smoke checklist

When you visit the running dashboard for the first time, the
following should be true:

- [ ] `GET /` returns HTML containing the project badge and one row
      per resource.
- [ ] The HTMX library is fetched from `/_assets/htmx.min.js` and the
      stylesheet from `/_assets/style.css`. Both responses include a
      `Cache-Control: public, max-age=3600` header.
- [ ] Stopping a container externally (for example, via the system
      Docker CLI) is reflected on the index within two seconds.
- [ ] Clicking **Restart** on a running resource shows the row going
      `running` → `starting` → `running` again without a full page
      reload.
- [ ] `GET /resources/{unknown}` returns `404 Not Found`.
- [ ] The detail log pane begins streaming as soon as the page loads
      and reports `[log stream closed]` when the resource is stopped.

When all six boxes are ticked, the dashboard is working as intended.
