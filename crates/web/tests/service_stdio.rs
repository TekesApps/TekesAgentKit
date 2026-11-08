use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct OwnedService(Child);
impl Drop for OwnedService {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[test]
fn malformed_uncorrelated_lines_do_not_stop_later_requests() {
    let mut child = OwnedService(
        Command::new(env!("CARGO_BIN_EXE_tekes-agent-web"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("private service binary"),
    );
    let mut stdin = child.0.stdin.take().unwrap();
    let stdout = child.0.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    writeln!(stdin, "{{\"endpoints\":[]}}\nnot-json").unwrap();
    writeln!(stdin, "{{\"op\":\"endpoints\"}}").unwrap();
    writeln!(stdin, "{{\"id\":\"recover\",\"op\":\"endpoints\"}}").unwrap();
    stdin.flush().unwrap();
    // EOF intentionally stops the private service and aborts outstanding work.
    // Keep the host pipe alive until the valid request has actually completed.
    let frames: Vec<Value> = (0..2)
        .map(|_| {
            let line = receiver
                .recv_timeout(Duration::from_secs(120))
                .expect("bounded service response")
                .expect("response line");
            serde_json::from_str(&line).unwrap()
        })
        .collect();
    assert_eq!(
        frames,
        vec![
            json!({"id":"recover","value":[]}),
            json!({"id":"recover","done":true}),
        ]
    );
    drop(stdin);
    assert!(child.0.wait().expect("owned service exit").success());
    reader.join().unwrap();
}
