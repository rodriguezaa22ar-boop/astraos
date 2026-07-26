use crate::{command_exists, DeveloperServicesSnapshot, ServiceStatus};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
pub(crate) struct DeveloperServicesCollector {
    timeout: Duration,
}

impl DeveloperServicesCollector {
    pub(crate) fn collect(&self) -> DeveloperServicesSnapshot {
        DeveloperServicesSnapshot {
            docker: docker_status(self.timeout),
            ollama: ollama_status(self.timeout),
        }
    }
}

impl Default for DeveloperServicesCollector {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_PROBE_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeOutcome {
    Reachable,
    Unreachable,
    Missing,
    TimedOut,
    InvalidResponse,
    Failed,
}

fn status_from_probe(outcome: ProbeOutcome) -> ServiceStatus {
    match outcome {
        ProbeOutcome::Reachable => ServiceStatus::Running,
        ProbeOutcome::Unreachable | ProbeOutcome::TimedOut => ServiceStatus::Stopped,
        ProbeOutcome::Missing => ServiceStatus::Unavailable,
        ProbeOutcome::InvalidResponse | ProbeOutcome::Failed => ServiceStatus::Unknown,
    }
}

fn docker_status(timeout: Duration) -> ServiceStatus {
    status_from_probe(run_docker_probe(timeout))
}

fn run_docker_probe(timeout: Duration) -> ProbeOutcome {
    let child = match Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return ProbeOutcome::Missing,
        Err(_) => return ProbeOutcome::Failed,
    };

    wait_for_child(child, timeout)
}

fn wait_for_child(mut child: Child, timeout: Duration) -> ProbeOutcome {
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return command_status_outcome(status),
            Ok(None) if Instant::now() < deadline => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return ProbeOutcome::TimedOut;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return ProbeOutcome::Failed;
            }
        }
    }
}

fn command_status_outcome(status: ExitStatus) -> ProbeOutcome {
    if status.success() {
        ProbeOutcome::Reachable
    } else {
        ProbeOutcome::Unreachable
    }
}

fn ollama_status(timeout: Duration) -> ServiceStatus {
    let outcome = probe_ollama(timeout);
    ollama_status_from_probe(outcome, command_exists("ollama"))
}

fn ollama_status_from_probe(outcome: ProbeOutcome, executable_available: bool) -> ServiceStatus {
    if matches!(outcome, ProbeOutcome::Unreachable | ProbeOutcome::TimedOut)
        && !executable_available
    {
        ServiceStatus::Unavailable
    } else {
        status_from_probe(outcome)
    }
}

fn probe_ollama(timeout: Duration) -> ProbeOutcome {
    let addresses = [
        SocketAddr::from(([127, 0, 0, 1], 11_434)),
        SocketAddr::new(std::net::Ipv6Addr::LOCALHOST.into(), 11_434),
    ];

    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(mut stream) => {
                if stream.set_read_timeout(Some(timeout)).is_err()
                    || stream.set_write_timeout(Some(timeout)).is_err()
                {
                    return ProbeOutcome::Failed;
                }

                let request =
                    b"GET /api/version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";

                if stream.write_all(request).is_err() {
                    return ProbeOutcome::Unreachable;
                }

                let mut response = [0_u8; 512];

                return match stream.read(&mut response) {
                    Ok(0) => ProbeOutcome::InvalidResponse,
                    Ok(length) => parse_http_status(&response[..length]),
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                        ) =>
                    {
                        ProbeOutcome::TimedOut
                    }
                    Err(_) => ProbeOutcome::Unreachable,
                };
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return ProbeOutcome::TimedOut;
            }
            Err(_) => {}
        }
    }

    ProbeOutcome::Unreachable
}

fn parse_http_status(response: &[u8]) -> ProbeOutcome {
    let Some(first_line) = response.split(|byte| *byte == b'\n').next() else {
        return ProbeOutcome::InvalidResponse;
    };
    let Ok(first_line) = std::str::from_utf8(first_line) else {
        return ProbeOutcome::InvalidResponse;
    };
    let mut parts = first_line.trim_end_matches('\r').split_whitespace();
    let protocol = parts.next();
    let status = parts.next().and_then(|value| value.parse::<u16>().ok());

    match (protocol, status) {
        (Some(protocol), Some(200)) if protocol.starts_with("HTTP/") => ProbeOutcome::Reachable,
        (Some(protocol), Some(_)) if protocol.starts_with("HTTP/") => ProbeOutcome::InvalidResponse,
        _ => ProbeOutcome::InvalidResponse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn service_status_mapping_is_explicit() {
        assert_eq!(
            status_from_probe(ProbeOutcome::Reachable),
            ServiceStatus::Running
        );
        assert_eq!(
            status_from_probe(ProbeOutcome::Unreachable),
            ServiceStatus::Stopped
        );
        assert_eq!(
            status_from_probe(ProbeOutcome::TimedOut),
            ServiceStatus::Stopped
        );
        assert_eq!(
            status_from_probe(ProbeOutcome::Missing),
            ServiceStatus::Unavailable
        );
        assert_eq!(
            status_from_probe(ProbeOutcome::InvalidResponse),
            ServiceStatus::Unknown
        );
        assert_eq!(
            status_from_probe(ProbeOutcome::Failed),
            ServiceStatus::Unknown
        );
    }

    #[test]
    fn parses_successful_ollama_http_response() {
        assert_eq!(
            parse_http_status(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"),
            ProbeOutcome::Reachable
        );
    }

    #[test]
    fn ollama_absence_and_reachability_are_mapped_independently() {
        assert_eq!(
            ollama_status_from_probe(ProbeOutcome::Unreachable, false),
            ServiceStatus::Unavailable
        );
        assert_eq!(
            ollama_status_from_probe(ProbeOutcome::TimedOut, false),
            ServiceStatus::Unavailable
        );
        assert_eq!(
            ollama_status_from_probe(ProbeOutcome::Reachable, false),
            ServiceStatus::Running
        );
        assert_eq!(
            ollama_status_from_probe(ProbeOutcome::Unreachable, true),
            ServiceStatus::Stopped
        );
    }

    #[test]
    fn rejects_non_success_and_malformed_http_responses() {
        assert_eq!(
            parse_http_status(b"HTTP/1.1 503 Service Unavailable\r\n\r\n"),
            ProbeOutcome::InvalidResponse
        );
        assert_eq!(
            parse_http_status(b"not http"),
            ProbeOutcome::InvalidResponse
        );
        assert_eq!(
            parse_http_status(&[0xff, 0xfe]),
            ProbeOutcome::InvalidResponse
        );
    }

    #[test]
    fn command_exit_status_maps_without_docker() {
        let success = Command::new("sh")
            .args(["-c", "exit 0"])
            .status()
            .expect("test shell should execute");
        let failure = Command::new("sh")
            .args(["-c", "exit 1"])
            .status()
            .expect("test shell should execute");

        assert_eq!(command_status_outcome(success), ProbeOutcome::Reachable);
        assert_eq!(command_status_outcome(failure), ProbeOutcome::Unreachable);
    }

    #[test]
    fn bounded_child_probe_times_out() {
        let child = Command::new("sh")
            .args(["-c", "exec sleep 1"])
            .spawn()
            .expect("test shell should execute");

        assert_eq!(
            wait_for_child(child, Duration::from_millis(10)),
            ProbeOutcome::TimedOut
        );
    }
}
