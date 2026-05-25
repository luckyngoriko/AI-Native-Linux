//! Tier-2 local filesystem/process/socket primitive executors.

use std::path::Path;

use serde_json::{json, Value};

use crate::{PrimitiveResult, VerificationPrimitive};

use super::{
    bool_actual, optional_str, optional_string_array, primitive_result, required_i32, required_str,
    required_u16, LocalProbe, ProbeVerdict,
};

/// Compare a local environment variable with an expected value.
pub async fn env_var_eq(probe: &dyn LocalProbe, expected: &Value) -> ProbeVerdict {
    let name = match required_str(expected, "name") {
        Ok(name) => name,
        Err(error) => return error,
    };
    let expected_value = match required_str(expected, "expected") {
        Ok(expected_value) => expected_value,
        Err(error) => return error,
    };
    let Some(value) = probe.env_var(name).await else {
        return ProbeVerdict::probe_error(format!("environment variable `{name}` is not set"));
    };
    let actual = json!({"value": value});

    if value == expected_value {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

/// Compare a local command exit code with `expected_exit_code`.
pub async fn command_exit_code_eq(probe: &dyn LocalProbe, expected: &Value) -> ProbeVerdict {
    let cmd = match required_str(expected, "cmd") {
        Ok(cmd) => cmd,
        Err(error) => return error,
    };
    let expected_exit_code = match required_i32(expected, "expected_exit_code") {
        Ok(expected_exit_code) => expected_exit_code,
        Err(error) => return error,
    };
    let args = match optional_string_array(expected, "args") {
        Ok(args) => args,
        Err(error) => return error,
    };
    let Some(exit_code) = probe.command_exit_code(cmd, &args).await else {
        return ProbeVerdict::probe_error(format!("command `{cmd}` did not produce an exit code"));
    };
    let actual = json!({"exit_code": exit_code});

    if exit_code == expected_exit_code {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

/// Execute a Tier-2 S2.4 primitive through a local probe.
pub async fn execute(
    primitive: VerificationPrimitive,
    expected: &Value,
    probe: &dyn LocalProbe,
) -> PrimitiveResult {
    let verdict = match primitive {
        VerificationPrimitive::ServiceActive => service_active(expected, probe).await,
        VerificationPrimitive::ServiceInactive => service_inactive(expected, probe).await,
        VerificationPrimitive::PackageInstalled => package_installed(expected, probe).await,
        VerificationPrimitive::PortOpen => port_open(expected, probe).await,
        VerificationPrimitive::PortClosed => port_closed(expected, probe).await,
        VerificationPrimitive::FileExists => file_exists(expected, probe).await,
        VerificationPrimitive::FileHash => file_hash(expected, probe).await,
        VerificationPrimitive::RepoExists => repo_exists(expected, probe).await,
        VerificationPrimitive::WebRendererBoundTo => web_renderer_bound_to(expected, probe).await,
        other => ProbeVerdict::probe_error(format!("{other} is not a Tier-2 primitive")),
    };

    primitive_result(primitive, expected, verdict)
}

async fn service_active(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    let service = match required_str(expected, "service") {
        Ok(service) => service,
        Err(error) => return error,
    };
    let running = probe.process_running(service).await;
    let actual = bool_actual("running", running);

    if running {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

async fn service_inactive(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    let service = match required_str(expected, "service") {
        Ok(service) => service,
        Err(error) => return error,
    };
    let running = probe.process_running(service).await;
    let actual = bool_actual("running", running);

    if running {
        ProbeVerdict::failed(actual)
    } else {
        ProbeVerdict::passed(actual)
    }
}

async fn package_installed(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    if expected.get("cmd").is_some() {
        return command_exit_code_eq(probe, expected).await;
    }

    let package = match required_str(expected, "package") {
        Ok(package) => package,
        Err(error) => return error,
    };
    let args = vec!["-W".to_owned(), package.to_owned()];
    let Some(exit_code) = probe.command_exit_code("dpkg-query", &args).await else {
        return ProbeVerdict::probe_error("dpkg-query did not produce an exit code");
    };
    let installed = exit_code == 0;
    let actual = json!({"exit_code": exit_code, "installed": installed});

    if installed {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

async fn port_open(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    let port = match local_tcp_port(expected) {
        Ok(port) => port,
        Err(error) => return error,
    };
    let listening = probe.port_listening(port).await;
    let actual = bool_actual("listening", listening);

    if listening {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

async fn port_closed(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    let port = match local_tcp_port(expected) {
        Ok(port) => port,
        Err(error) => return error,
    };
    let listening = probe.port_listening(port).await;
    let actual = bool_actual("listening", listening);

    if listening {
        ProbeVerdict::failed(actual)
    } else {
        ProbeVerdict::passed(actual)
    }
}

async fn file_exists(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    let path = match required_str(expected, "object_or_path") {
        Ok(path) => path,
        Err(error) => return error,
    };
    let exists = probe.file_exists(path).await;
    let actual = bool_actual("exists", exists);

    if exists {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

async fn file_hash(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    let path = match required_str(expected, "object_or_path") {
        Ok(path) => path,
        Err(error) => return error,
    };
    let expected_hash = match required_str(expected, "expected_hash_hex") {
        Ok(expected_hash) => expected_hash,
        Err(error) => return error,
    };
    let Some(observed_hash) = probe.file_blake3(path).await else {
        return ProbeVerdict::probe_error(format!("could not hash local path `{path}`"));
    };
    let actual = json!({"observed_hash": observed_hash});

    if observed_hash == expected_hash {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

async fn repo_exists(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    let path = match required_str(expected, "path_or_object") {
        Ok(path) => path,
        Err(error) => return error,
    };
    let git_dir = Path::new(path).join(".git");
    let Some(git_dir) = git_dir.to_str() else {
        return ProbeVerdict::probe_error("repository path is not valid UTF-8");
    };
    let exists = probe.file_exists(git_dir).await;
    let actual = bool_actual("git_dir_exists", exists);

    if exists {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

async fn web_renderer_bound_to(expected: &Value, probe: &dyn LocalProbe) -> ProbeVerdict {
    let host = match required_str(expected, "host") {
        Ok(host) => host,
        Err(error) => return error,
    };
    if !is_loopback_host(host) {
        return ProbeVerdict::probe_error(
            "web_renderer_bound_to only supports local loopback hosts in M8",
        );
    }
    let port = match required_u16(expected, "port") {
        Ok(port) => port,
        Err(error) => return error,
    };
    let listening = probe.port_listening(port).await;
    let actual = json!({
        "observed_host": host,
        "observed_port": port,
        "lan_exposed": false,
        "listening": listening,
    });

    if listening {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

fn local_tcp_port(expected: &Value) -> Result<u16, ProbeVerdict> {
    let protocol = optional_str(expected, "protocol").unwrap_or("tcp");
    if protocol != "tcp" {
        return Err(ProbeVerdict::probe_error(
            "only TCP port probes are implemented in M8",
        ));
    }
    let host = optional_str(expected, "host").unwrap_or("127.0.0.1");
    if !is_loopback_host(host) {
        return Err(ProbeVerdict::probe_error(
            "only loopback port probes are implemented in M8",
        ));
    }
    required_u16(expected, "port")
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}
