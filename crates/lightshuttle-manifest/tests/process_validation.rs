//! Validation tests for the `process` resource kind: the three refusals a
//! `process` declaration introduces, plus a control case that must be
//! accepted.

use lightshuttle_manifest::Manifest;

fn reference_to_portless_process(property: &str) -> String {
    format!(
        r#"
project:
  name: app
resources:
  worker:
    process:
      command: ["node", "server.js"]
  api:
    container:
      image: alpine
      env:
        WORKER_TARGET: "${{resources.worker.{property}}}"
"#
    )
}

#[test]
fn reference_to_host_of_a_portless_process_is_refused() {
    let yaml = reference_to_portless_process("host");
    let err = Manifest::parse(&yaml)
        .expect_err("a `.host` reference to a portless process must be refused");
    let message = err.to_string();
    assert!(
        message.contains("api") && message.contains("worker"),
        "the message must name both the consumer `api` and the referenced resource `worker`, got: {message}"
    );
}

#[test]
fn reference_to_url_of_a_portless_process_is_refused() {
    let yaml = reference_to_portless_process("url");
    let err = Manifest::parse(&yaml)
        .expect_err("a `.url` reference to a portless process must be refused");
    let message = err.to_string();
    assert!(
        message.contains("api") && message.contains("worker"),
        "the message must name both the consumer `api` and the referenced resource `worker`, got: {message}"
    );
}

#[test]
fn reference_to_port_of_a_portless_process_is_refused() {
    let yaml = reference_to_portless_process("port");
    let err = Manifest::parse(&yaml)
        .expect_err("a `.port` reference to a portless process must be refused");
    let message = err.to_string();
    assert!(
        message.contains("api") && message.contains("worker"),
        "the message must name both the consumer `api` and the referenced resource `worker`, got: {message}"
    );
}

#[test]
fn process_depending_on_a_portless_container_is_refused() {
    let yaml = r"
project:
  name: app
resources:
  db:
    container:
      image: alpine
  worker:
    process:
      command: ['node', 'server.js']
      depends_on: [db]
";
    let err = Manifest::parse(yaml)
        .expect_err("a process depending on a container that publishes no port must be refused");
    let message = err.to_string();
    assert!(
        message.contains("worker") && message.contains("db"),
        "the message must name both the process `worker` and the container `db`, got: {message}"
    );
}

#[test]
fn empty_process_command_is_refused() {
    let yaml = r"
project:
  name: app
resources:
  worker:
    process:
      command: []
";
    let err = Manifest::parse(yaml).expect_err("an empty process command must be refused");
    let message = err.to_string();
    assert!(
        message.contains("worker"),
        "the message must name the resource `worker`, got: {message}"
    );
}

/// Control case: a reference to a process that does declare a `port` must be
/// accepted. Without this test, the three refusals above could be satisfied
/// by rejecting every process reference outright, port or not.
#[test]
fn reference_to_host_of_a_process_with_a_port_is_accepted() {
    let yaml = r#"
project:
  name: app
resources:
  worker:
    process:
      command: ["node", "server.js"]
      port: 4000
  api:
    container:
      image: alpine
      env:
        WORKER_HOST: "${resources.worker.host}"
"#;
    Manifest::parse(yaml).expect("a reference to a process that declares a port must be accepted");
}
