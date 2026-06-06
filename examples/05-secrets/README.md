# Example 05: secrets from a `.env` file

A Postgres database whose password is **required** from the
environment, and a container with an **optional** token that falls
back to a default. Demonstrates the v0.4.0 secrets workflow:

- `${env.DEMO_DB_PASSWORD}` without a default: the stack refuses to
  boot while the variable is missing.
- `${env.DEMO_API_TOKEN:-dev-token}` with a default: always resolves.
- `.env` file precedence over the process environment.
- `lightshuttle secrets check` as a pre-boot audit.

## Run it

```sh
cd examples/05-secrets
cp .env.example .env
lightshuttle secrets check
```

The check reports every referenced variable with its status and
source:

```
secrets for project `secrets-demo`:

  DEMO_DB_PASSWORD                 set (.env)
  DEMO_API_TOKEN                   default (dev-token)

all required secrets are set
```

Then boot:

```sh
lightshuttle up
```

## See the fail-fast behaviour

Delete the `.env` file and run the check again:

```sh
rm .env
lightshuttle secrets check
```

`DEMO_DB_PASSWORD` is reported `missing` and the command exits
non-zero. `lightshuttle up` applies the same rule and refuses to boot,
naming the missing variable, so a misconfigured stack never
half-starts. Restore the file with `cp .env.example .env`.

Both commands accept `--env-file <path>` to point at another file,
which then must exist.
