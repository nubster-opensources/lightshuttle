# Control plane REST API

The local control plane exposes a minimal REST surface on
`127.0.0.1:<port>` (port selection: `--control-port` flag, then
`dashboard.port` from the manifest, then a random free port picked by
the OS). All routes that produce JSON share the shapes defined by the
`ResourceView` value type in `lightshuttle-runtime`.

## OpenAPI fragment (v0.2.0)

```yaml
openapi: 3.0.3
info:
  title: LightShuttle control plane
  version: "0.2.0"
  description: |
    Local-only HTTP API consumed by the dashboard, the `lightshuttle`
    CLI client subcommands and any operator-facing tooling. The server
    binds on `127.0.0.1` exclusively.

paths:
  /healthz:
    get:
      summary: Liveness probe
      responses:
        "200":
          description: Server is up.
          content:
            application/json:
              schema:
                type: object
                required: [status, project]
                properties:
                  status:
                    type: string
                    enum: [ok]
                  project:
                    type: string

  /api/resources:
    get:
      summary: List every managed resource
      responses:
        "200":
          description: Resource views in topological order.
          content:
            application/json:
              schema:
                type: array
                items:
                  $ref: "#/components/schemas/ResourceView"

  /api/resources/{name}:
    get:
      summary: Fetch a single resource view
      parameters:
        - in: path
          name: name
          required: true
          schema:
            type: string
      responses:
        "200":
          description: Resource view.
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/ResourceView"
        "404":
          description: No resource with that name exists in the current plan.
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/ApiError"

components:
  schemas:
    ResourceView:
      type: object
      required: [name, kind, status, healthy, image]
      properties:
        name:
          type: string
          description: Manifest-declared resource name.
        kind:
          type: string
          description: Resource kind discriminant (`postgres`, `redis`, `container`, `dockerfile`).
        status:
          $ref: "#/components/schemas/ResourceStatus"
        healthy:
          type: boolean
          description: Whether the resource passed its healthcheck.
        image:
          type: string
          description: Container image reference, as resolved at start time.
        started_at:
          type: object
          nullable: true
          description: Wall-clock time at which the runtime accepted the start request.
        last_error:
          type: string
          nullable: true
          description: Last terminal failure reason, when applicable.

    ResourceStatus:
      type: string
      enum: [Pending, Starting, Running, Failed, Stopped]

    ApiError:
      type: object
      required: [error]
      properties:
        error:
          type: string
          description: Short, machine-friendly slug describing the error category.
        resource:
          type: string
          nullable: true
          description: Resource name when the error is scoped to a single resource.
```

## Status mapping reference

| `LifecycleHandleError` variant | HTTP status | `error` body slug          |
| ------------------------------ | ----------- | -------------------------- |
| `UnknownResource(name)`        | `404`       | `unknown resource`         |
| `NotSupported(op)`             | `501`       | `operation \`<op>\` is not supported yet` |
| `Runtime(_)`                   | `500`       | original runtime error message |
